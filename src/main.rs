use anyhow::{Context, Result, anyhow};
use chrono::Local;
use core::f64;
use midir::{MidiOutput, os::unix::VirtualOutput};
use nusb::{
    MaybeFuture, list_devices,
    transfer::{Bulk, In, Out},
};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const MASCHINE_MK1_VID: u16 = 0x17cc;
const MASCHINE_MK1_PID: u16 = 0x0808;

const CHAN_1_NOTE_ON: u8 = 0x90;
const CHAN_1_NOTE_OFF: u8 = 0x80;
const CHAN_1_POLYPHONIC_AFTERTOUCH: u8 = 0xA0;

const CC_PREFIX: u8 = 0xB0;
const CC_CHANNEL: u8 = 1;
const CC_MAPPING: [u8; 11] = [
    77, // 0 = right display, dial 4
    73, // 1 = left display, dial 4
    79, // 2 = swing
    76, // 3 = right display, dial 3
    72, // 4 = left display, dial 3
    78, // 5 = tempo
    75, // 6 = right display, dial 2
    71, // 7 = left display, dial 2
    7,  // 8 = volume
    74, // 9 = right display, dial 1
    70, // 10 = left display, dial 1
];

fn main() -> Result<()> {
    // a minor starting at A0
    // i = pad  = delta
    let pad_notes: Vec<u8> = vec![
        53, //  0 = 13 : = 0
        55, //  1 = 14 : + 2
        57, //  2 = 15 : + 2
        59, //  3 = 16 : + 2
        47, //  4 = 9  : - 12
        48, //  5 = 10 : + 2
        50, //  6 = 11 : + 2
        52, //  7 = 12 : + 2
        40, //  8 = 5  : - 12
        41, //  9 = 6  : + 2
        43, // 10 = 7  : + 2
        45, // 11 = 8  : + 2
        33, // 12 = 1  : - 12
        35, // 13 = 2  : + 2
        36, // 14 = 3  : + 2
        38, // 15 = 4  : + 2
    ];

    // compatible with bespoke synth drumplayer
    // let pad_notes: Vec<u8> = vec![
    //     36, //  0 = 13 : = 0
    //     37, //  1 = 14 : + 2
    //     38, //  2 = 15 : + 2
    //     39, //  3 = 16 : + 2
    //     32, //  4 = 9  : - 12
    //     33, //  5 = 10 : + 2
    //     34, //  6 = 11 : + 2
    //     35, //  7 = 12 : + 2
    //     28, //  8 = 5  : - 12
    //     29, //  9 = 6  : + 2
    //     30, // 10 = 7  : + 2
    //     31, // 11 = 8  : + 2
    //     24, // 12 = 1  : - 12
    //     25, // 13 = 2  : + 2
    //     26, // 14 = 3  : + 2
    //     27, // 15 = 4  : + 2
    // ];

    //
    // setup midi stuff
    //

    let midi_out = MidiOutput::new("rust-maschine-mk1-midi-out")?;

    // TODO: need this kinda weird handling of this due to differences between macOS and Linux
    //   re: the error not being Send
    let midi_output_conn = Arc::new(Mutex::new(
        match midi_out.create_virtual("rust-maschine-mk1-midi-out-virtual") {
            Ok(midi_output_conn) => Ok(midi_output_conn),
            Err(err) => Err(anyhow!("some kinda error: {:?}", err)),
        }
        .context("failed to create virtual midi port")?,
    ));

    //
    // handle midi for dials
    //

    let midi_output_conn_for_send_dial = midi_output_conn.clone();

    let value_by_cc: Arc<Mutex<HashMap<u8, u8>>> = Arc::new(Mutex::new(HashMap::new()));

    let send_dial = move |dial: usize, value: u8| -> Result<()> {
        let now = Local::now();

        assert!(
            dial < 12,
            "{now} | send_dial; dial out of spec- dial: {dial}, value: {value}",
        );

        let cc = CC_MAPPING[dial];

        assert!(
            value < 128,
            "{now} | send_dial; value out of spec- cc: {cc}, value: {value}",
        );

        let mut midi_output_conn = midi_output_conn_for_send_dial
            .lock()
            .ok()
            .context("failed to lock mutex")?;

        let last_value = {
            let locked_value_by_cc = value_by_cc
                .lock()
                .ok()
                .context("send_dial: failed to lock value_by_cc")?;

            match locked_value_by_cc.get(&cc) {
                Some(last_value) => *last_value,
                None => 255, // 255 is a sentinel because it's not possible
            }
        };

        if last_value != value {
            println!(
                "{now} | send_dial; cc: {cc}, last_value: {last_value}, value: {value}, changed: {}",
                last_value != value
            );
            midi_output_conn
                .send(&[CC_PREFIX + CC_CHANNEL, cc, value])
                .context("send_pad: note off failed")?;
        }

        {
            let mut locked_value_by_cc = value_by_cc
                .lock()
                .ok()
                .context("send_dial: failed to lock value_by_cc")?;

            locked_value_by_cc.insert(cc, value);
        };

        Ok(())
    };

    //
    // handle midi for pads
    //

    let midi_output_conn_for_send_pads = midi_output_conn.clone();

    let pad_is_on_by_note: Arc<Mutex<HashMap<u8, bool>>> = Arc::new(Mutex::new(HashMap::new()));

    let velocity_by_note: Arc<Mutex<HashMap<u8, u8>>> = Arc::new(Mutex::new(HashMap::new()));

    let send_pad = move |note: u8, velocity: u8, desired_is_on: bool| -> Result<()> {
        let now = Local::now();

        assert!(
            note < 128,
            "{now} | send pad; note out of spec- note: {note}, velocity: {velocity}, desired_is_on: {desired_is_on}",
        );

        let mut midi_output_conn = midi_output_conn_for_send_pads
            .lock()
            .ok()
            .context("failed to lock mutex")?;

        let mut actual_is_on = {
            let locked_pad_is_on_by_note = pad_is_on_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock pad_is_on_by_note")?;

            match locked_pad_is_on_by_note.get(&note) {
                Some(last_is_on) => *last_is_on,
                None => false,
            }
        };

        let last_velocity = {
            let locked_velocity_by_note = velocity_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock velocity_by_note")?;

            match locked_velocity_by_note.get(&note) {
                Some(last_velocity) => *last_velocity,
                None => 255, // 255 is a sentinel because it's not possible
            }
        };

        if desired_is_on {
            if actual_is_on {
                if last_velocity != velocity {
                    println!(
                        "{now} | send_pad; state: aftertouch, note: {note}, last_velocity: {last_velocity}, velocity: {velocity}, changed: {}",
                        last_velocity != velocity
                    );
                    midi_output_conn
                        .send(&[CHAN_1_POLYPHONIC_AFTERTOUCH, note, velocity])
                        .context("send_pad: note aftertouch failed")?;
                }
            } else {
                println!("{now} | send_pad; state: on, note: {note}, velocity: {velocity}");
                midi_output_conn
                    .send(&[CHAN_1_NOTE_ON, note, velocity])
                    .context("send_pad: note on failed")?;

                actual_is_on = true;
            }
        } else {
            if actual_is_on {
                println!("{now} | send_pad; state: off, note: {note}, velocity: {velocity}");
                midi_output_conn
                    .send(&[CHAN_1_NOTE_OFF, note, velocity])
                    .context("send_pad: note off failed")?;

                actual_is_on = false;
            };
        }

        {
            let mut locked_pad_is_on_by_note = pad_is_on_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock pad_is_on_by_note")?;

            locked_pad_is_on_by_note.insert(note, actual_is_on);
        };

        {
            let mut locked_velocity_by_note = velocity_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock velocity_by_note")?;

            locked_velocity_by_note.insert(note, velocity);
        };

        Ok(())
    };

    //
    // try to find the device and dump out some info for it
    //

    let device_info = list_devices()
        .wait()?
        .find(|dev| dev.vendor_id() == MASCHINE_MK1_VID && dev.product_id() == MASCHINE_MK1_PID)
        .context("failed to find device")?;
    println!("device_info: {:?}", device_info);
    println!();

    let device = device_info.open().wait().context("failed to open device")?;

    for configuration in device.configurations() {
        println!("{:?}", configuration);
        println!();
    }

    //
    // now try to talk to it
    //

    device
        .set_configuration(1)
        .wait()
        .context("failed to set configuration")?;

    let claimed_interface = device
        .claim_interface(0)
        .wait()
        .context("failed to claim interface")?;

    claimed_interface
        .set_alt_setting(1)
        .wait()
        .context("failed to set alt setting")?;

    let mut control_writer = claimed_interface
        .endpoint::<Bulk, Out>(0x01)
        .context("failed to get control writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let _display_writer = claimed_interface
        .endpoint::<Bulk, Out>(0x08)
        .context("failed to get display writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut buttons_and_dials_reader = claimed_interface
        .endpoint::<Bulk, In>(0x81)
        .context("failed to get buttons and dials reader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut pads_reader = claimed_interface
        .endpoint::<Bulk, In>(0x84)
        .context("failed to get pad sreader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    //
    // seems like you have to regularly send this or nothing works, heartbeat? idk
    //

    let control_writer_thread = thread::spawn(move || -> Result<()> {
        loop {
            let buf = [0x0B, 0xFF, 0x02, 0x05];

            match control_writer.write(&buf) {
                Ok(_) => {}
                Err(err) => {
                    println!("control_writer; err: {:?}", err)
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    });

    //
    // handle buttons and dials input
    //

    let mut buttons: Vec<bool> = vec![];
    for _ in 0..42 {
        buttons.push(false);
    }

    let mut last_raw_dials: Vec<u8> = vec![];
    for _ in 0..22 {
        last_raw_dials.push(0);
    }

    let mut angles: Vec<f64> = vec![];
    for _ in 0..11 {
        angles.push(0.0);
    }

    let mut last_angles: Vec<f64> = vec![];
    for _ in 0..11 {
        last_angles.push(0.0);
    }

    let mut dials: Vec<u8> = vec![];
    for _ in 0..11 {
        dials.push(0);
    }

    let buttons_and_dials_reader_thread = thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_millis(1));

            let mut buttons_and_dials_read_buf: Vec<u8> = vec![0; 512];

            match buttons_and_dials_reader.read(&mut buttons_and_dials_read_buf) {
                Ok(n) => {
                    let raw_msg = buttons_and_dials_read_buf[0..n].to_vec();

                    // not sure what this message is- heartbeat maybe?
                    if raw_msg.len() <= 1 {
                        continue;
                    }

                    //
                    // handle buttons input
                    //

                    if raw_msg.len() == 8 && raw_msg.last() != Some(&0) {
                        //
                        // decoding
                        //

                        // values are 1-byte bitmasks (each bit is a button state)
                        for (i, x) in raw_msg.iter().enumerate() {
                            for j in 0..8 {
                                let mut b = j + (i * 8);

                                let n = (x >> j) & 1;
                                let pressed = n > 0;

                                // first 8 bytes don't seem to change (framing? idk)
                                // first 8 bytes of last 16 bytes has only one button it
                                // last 8 bytes seem to indicate pressed / release vs a poll-based refresh / update (triggered by the writer I think)
                                if b <= 7 || b >= 50 {
                                    continue;
                                }

                                b -= 8;

                                buttons[b] = pressed;
                            }
                        }

                        //
                        // sending midi
                        //

                        for _n in buttons.iter() {
                            // let res = send_dial(i, *n);
                            // if res.is_err() {
                            //     println!("{:?}", res);
                            // }
                        }
                    }

                    //
                    // handle dials input
                    //

                    if raw_msg.len() == 33 {
                        //
                        // decoding
                        //

                        // values are little-endian u16 (each 2-byte chunks is a pad state)
                        // first byte doesn't seem important
                        // last 10 bytes don't seem important
                        let raw_dials = &raw_msg[1..23];

                        if raw_dials.ne(&last_raw_dials) {
                            for (i, x) in raw_dials.chunks(2).enumerate() {
                                assert!(
                                    x.len() == 2,
                                    "got length {} for raw_dials[{i}] instead of length 2",
                                    x.len()
                                );

                                let x0 = (x[0] as f64 / 127.0) - 1.0; // normalize
                                let x1 = (x[1] as f64 / 127.0) - 1.0; // normalize

                                let a = x0.atan2(x1) / f64::consts::PI;
                                let last_a = last_angles[i];
                                let d = a - last_a;

                                last_angles[i] = a;

                                //
                                // this confusing block is all about the overflow
                                //

                                let mut b = a;
                                let mut last_b = last_a;
                                let mut e = d;

                                if d >= 1.0 {
                                    last_b = -last_b;
                                    e = b - last_b;
                                    if e >= 0.0 {
                                        e = -e;
                                    }
                                } else if d < -1.0 {
                                    b = -b;
                                    e = b - last_b;
                                    if e < 0.0 {
                                        e = -e;
                                    }
                                }

                                let f = ((-e / 2.0) * 255.0) as i8; // re-scale to midi range (0 -> 127 in this case)

                                let mut g = dials[i] as i8;
                                g = g.saturating_add(f);
                                if g < 0 {
                                    g = 0;
                                }

                                dials[i] = g as u8;
                            }
                        }

                        last_raw_dials = raw_dials.into();

                        //
                        // sending midi
                        //

                        for (i, n) in dials.iter().enumerate() {
                            let res = send_dial(i, *n);
                            if res.is_err() {
                                println!("{:?}", res);
                            }
                        }
                    }
                }
                Err(_err) => {}
            }
        }
    });

    //
    // handle pads input
    //

    let mut pads: Vec<u8> = vec![];
    for _ in 0..16 {
        pads.push(0);
    }

    let pads_reader_thread = thread::spawn(move || -> Result<()> {
        loop {
            thread::sleep(Duration::from_millis(1));

            let mut pads_read_buf: Vec<u8> = vec![0; 512];
            match pads_reader.read(&mut pads_read_buf) {
                Ok(n) => {
                    let raw_msg = pads_read_buf[0..n].to_vec();

                    //
                    // decoding
                    //

                    // values are little-endian u16 (each 2-byte chunks is a pad state)
                    for x in raw_msg.chunks(2) {
                        assert!(x.len() == 2);

                        let raw_n = u16::from_le_bytes([x[0], x[1]]);

                        // the top 4 bits is the pad number
                        let i = raw_n >> 12;

                        // the bottom 12 bits is the value
                        let mut n = (raw_n & (4096 - 1)) as f64;
                        n /= 4095.0; // normalize 
                        n *= 255.0 / 2.0; // re-scale to midi range (0 -> 127 in this case)

                        assert!(n <= 127.0);

                        pads[i as usize] = n as u8;
                    }

                    //
                    // sending midi
                    //

                    for (i, n) in pads.iter().enumerate() {
                        let res = send_pad(pad_notes[i], *n, *n > 0);
                        if res.is_err() {
                            println!("{:?}", res);
                        }
                    }
                }
                Err(_err) => {}
            }
        }
    });

    //
    // just wait for something to explode
    //

    loop {
        thread::sleep(Duration::from_millis(1000));

        if control_writer_thread.is_finished() {
            return Err(anyhow!(
                "control writer thread finished: {:?}",
                control_writer_thread.join()
            ));
        }

        if buttons_and_dials_reader_thread.is_finished() {
            return Err(anyhow!(
                "buttons and dials reader thread finished: {:?}",
                buttons_and_dials_reader_thread.join()
            ));
        }

        if pads_reader_thread.is_finished() {
            return Err(anyhow!(
                "pads reader thread finished: {:?}",
                pads_reader_thread.join()
            ));
        }
    }
}
