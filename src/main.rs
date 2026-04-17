use anyhow::{Context, Result, anyhow};
use device::{pads::PadState, dials::DialDecoder, buttons::ButtonState, Endpoints, PAD_NOTES, CC_MAPPING};
use midi::MidiPort;
use nusb::{list_devices, transfer::Bulk, MaybeFuture};
use std::{io::{Read, Write}, thread, time::Duration};

mod device;
mod midi;

fn main() -> Result<()> {
    //
    // setup midi stuff
    //

    let midi_output = MidiPort::new("rust-maschine-mk1-midi-out", "rust-maschine-mk1-midi-out-virtual")?;

    //
    // find the device
    //

    let device_info = list_devices()
        .wait()?
        .find(|dev| {
            dev.vendor_id() == device::MASCHINE_MK1_VID
                && dev.product_id() == device::MASCHINE_MK1_PID
        })
        .context("failed to find Maschine Mk1 device")?;
    println!("device_info: {:?}", device_info);
    println!();

    let device = device_info.open().wait().context("failed to open device")?;

    for configuration in device.configurations() {
        println!("{configuration:?}");
        println!();
    }

    //
    // configure USB interface
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

    let eps = Endpoints::default_maschine_mk1();

    let mut heartbeat_writer = claimed_interface
        .endpoint::<Bulk, nusb::transfer::Out>(eps.heartbeat_out)
        .context("failed to get heartbeat writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let _display_writer = claimed_interface
        .endpoint::<Bulk, nusb::transfer::Out>(eps.display_out)
        .context("failed to get display writer")?
        .writer(512)
        .with_write_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut buttons_and_dials_reader = claimed_interface
        .endpoint::<Bulk, nusb::transfer::In>(eps.buttons_and_dials_in)
        .context("failed to get buttons and dials reader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    let mut pads_reader = claimed_interface
        .endpoint::<Bulk, nusb::transfer::In>(eps.pads_in)
        .context("failed to get pads reader")?
        .reader(512)
        .with_read_timeout(Duration::from_millis(10))
        .with_num_transfers(1);

    //
    // heartbeat thread (keeps device alive)
    //

    let heartbeat_thread = thread::spawn(move || -> Result<()> {
        loop {
            let buf = [0x0B, 0xFF, 0x02, 0x05];
            match heartbeat_writer.write(&buf) {
                Ok(_) => {}
                Err(err) => {
                    println!("heartbeat_writer; err: {err:?}");
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    //
    // pads thread
    //

    let pads_state = std::sync::Arc::new(std::sync::Mutex::new(PadState::new()));
    let pads_midi = midi_output.clone();

    let pads_thread = {
        let pads_state = pads_state.clone();
        let pads_midi = pads_midi.clone();

        thread::spawn(move || -> Result<()> {
            loop {
                thread::sleep(Duration::from_millis(1));

                let mut buf = [0u8; 512];
                match pads_reader.read(&mut buf) {
                    Ok(n) => {
                        let raw = &buf[..n];

                        let mut pads = pads_state.lock().unwrap();
                        pads.decode(raw);

                        for (i, &val) in pads.values().iter().enumerate() {
                            let _ = pads_midi.send_pad(PAD_NOTES[i], val, val > 0);
                        }
                    }
                    Err(_) => {}
                }
            }
        })
    };

    //
    // buttons + dials thread
    //

    let buttons_state = std::sync::Arc::new(std::sync::Mutex::new(ButtonState::new()));
    let dials_decoder = std::sync::Arc::new(std::sync::Mutex::new(DialDecoder::new()));
    let dials_midi = midi_output.clone();

    let buttons_and_dials_thread = {
        let buttons_state = buttons_state.clone();
        let dials_decoder = dials_decoder.clone();
        let dials_midi = dials_midi.clone();

        thread::spawn(move || -> Result<()> {
            let mut last_raw_dials: [u8; 22] = [0; 22];

            loop {
                thread::sleep(Duration::from_millis(1));

                let mut buf = [0u8; 512];
                match buttons_and_dials_reader.read(&mut buf) {
                    Ok(n) => {
                        let raw = &buf[..n];

                        // Heartbeat / too short — skip
                        if raw.len() <= 1 {
                            continue;
                        }

                        //
                        // decode buttons (8-byte messages)
                        //

                        if raw.len() == 8 && raw.last() != Some(&0) {
                            let mut buttons = buttons_state.lock().unwrap();
                            let _ = buttons.decode(raw);
                        }

                        //
                        // decode dials (33-byte messages, raw dial data at offset 1)
                        //

                        if raw.len() == 33 {
                            let raw_dials = &raw[1..23];

                            // Only decode if raw data changed
                            let mut decoder = dials_decoder.lock().unwrap();
                            if raw_dials != last_raw_dials {
                                if let Some(new_values) = decoder.decode(raw_dials) {
                                    for (i, &val) in new_values.iter().enumerate() {
                                        let _ = dials_midi.send_dial(i, val, &CC_MAPPING);
                                    }
                                }
                                last_raw_dials.copy_from_slice(raw_dials);
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        })
    };

    //
    // main loop — wait for threads to die
    //

    loop {
        thread::sleep(Duration::from_millis(1000));

        if heartbeat_thread.is_finished() {
            return Err(anyhow!(
                "heartbeat thread finished: {:?}",
                heartbeat_thread.join()
            ));
        }

        if buttons_and_dials_thread.is_finished() {
            return Err(anyhow!(
                "buttons and dials thread finished: {:?}",
                buttons_and_dials_thread.join()
            ));
        }

        if pads_thread.is_finished() {
            return Err(anyhow!(
                "pads thread finished: {:?}",
                pads_thread.join()
            ));
        }
    }
}
