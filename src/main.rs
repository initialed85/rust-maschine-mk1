use anyhow::{Context, Result, anyhow};
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

use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, VPos};
use plotters_backend::{
    BackendColor, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};

//
// <plotters>
//

#[derive(Copy, Clone)]
enum PixelState {
    Empty,
    HLine,
    VLine,
    Cross,
    Pixel,
    Text(char),
    Circle(bool),
}

impl PixelState {
    fn to_char(self) -> char {
        match self {
            Self::Empty => ' ',
            Self::HLine => '-',
            Self::VLine => '|',
            Self::Cross => '+',
            Self::Pixel => '.',
            Self::Text(c) => c,
            Self::Circle(filled) => {
                if filled {
                    '@'
                } else {
                    'O'
                }
            }
        }
    }

    fn update(&mut self, new_state: PixelState) {
        let next_state = match (*self, new_state) {
            (Self::HLine, Self::VLine) => Self::Cross,
            (Self::VLine, Self::HLine) => Self::Cross,
            (_, Self::Circle(what)) => Self::Circle(what),
            (Self::Circle(what), _) => Self::Circle(what),
            (_, Self::Pixel) => Self::Pixel,
            (Self::Pixel, _) => Self::Pixel,
            (_, new) => new,
        };

        *self = next_state;
    }
}

pub struct TextDrawingBackend(Vec<PixelState>);

impl DrawingBackend for TextDrawingBackend {
    type ErrorType = std::io::Error;

    fn get_size(&self) -> (u32, u32) {
        (200, 30)
    }

    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<std::io::Error>> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), DrawingErrorKind<std::io::Error>> {
        for r in 0..30 {
            let mut buf = String::new();
            for c in 0..200 {
                buf.push(self.0[r * 200 + c].to_char());
            }
            println!("{}", buf);
        }

        Ok(())
    }

    fn draw_pixel(
        &mut self,
        pos: (i32, i32),
        color: BackendColor,
    ) -> Result<(), DrawingErrorKind<std::io::Error>> {
        if color.alpha > 0.3 {
            self.0[(pos.1 * 200 + pos.0) as usize].update(PixelState::Pixel);
        }
        Ok(())
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        style: &S,
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        if from.0 == to.0 {
            let x = from.0;
            let y0 = from.1.min(to.1);
            let y1 = from.1.max(to.1);
            for y in y0..y1 {
                // self.0[(y * 200 + x) as usize].update(PixelState::VLine);
                self.0[(y * 200 + x) as usize].update(PixelState::Pixel);
            }
            return Ok(());
        }

        if from.1 == to.1 {
            let y = from.1;
            let x0 = from.0.min(to.0);
            let x1 = from.0.max(to.0);
            for x in x0..x1 {
                // self.0[(y * 200 + x) as usize].update(PixelState::HLine);
                self.0[(y * 200 + x) as usize].update(PixelState::Pixel);
            }
            return Ok(());
        }

        plotters_backend::rasterizer::draw_line(self, from, to, style)
    }

    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        _: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        Ok((text.len() as u32, 1))
    }

    fn draw_text<S: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &S,
        pos: (i32, i32),
    ) -> Result<(), DrawingErrorKind<Self::ErrorType>> {
        let (width, height) = self.estimate_text_size(text, style)?;
        let (width, height) = (width as i32, height as i32);
        let dx = match style.anchor().h_pos {
            HPos::Left => 0,
            HPos::Right => -width,
            HPos::Center => -width / 2,
        };
        let dy = match style.anchor().v_pos {
            VPos::Top => 0,
            VPos::Center => -height / 2,
            VPos::Bottom => -height,
        };
        let offset = (pos.1 + dy).max(0) * 200 + (pos.0 + dx).max(0);
        for (idx, chr) in (offset..).zip(text.chars()) {
            self.0[idx as usize].update(PixelState::Text(chr));
        }
        Ok(())
    }
}

fn draw_chart<DB: DrawingBackend>(
    b: DrawingArea<DB, plotters::coord::Shift>,
    x0_series: Vec<(f64, f64)>,
    x1_series: Vec<(f64, f64)>,
    angle_series: Vec<(f64, f64)>,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let mut chart = ChartBuilder::on(&b)
        .margin(1)
        .build_cartesian_2d(0.0..255.0, 0.0..255.0)?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw()?;

    chart.draw_series(LineSeries::new(x0_series, &RED))?;

    chart.draw_series(LineSeries::new(x1_series, &BLUE))?;

    // chart.draw_series(LineSeries::new(angle_series, &GREEN))?;

    print!("{}[2J", 27 as char);
    b.present()?;

    Ok(())
}

//
// </plotters>
//

const MASCHINE_MK1_VID: u16 = 0x17cc;
const MASCHINE_MK1_PID: u16 = 0x0808;

const CHAN_1_NOTE_ON: u8 = 0x90;
const CHAN_1_NOTE_OFF: u8 = 0x80;
const CHAN_1_POLYPHONIC_AFTERTOUCH: u8 = 0xA0;

const CC_PREFIX: u8 = 0xB0;
const CC_CHANNEL: u8 = 1;
const CC_74: u8 = 74;

fn main() -> Result<()> {
    // a minor starting at A0
    let pad_notes: Vec<u8> = vec![
        //  i = pad  = delta
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
    //         //  i = pad  = delta
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
    // setup MIDI stuff
    //

    let midi_out = MidiOutput::new("rust-maschine-mk1-midi-out")?;

    let midi_output_conn = Arc::new(Mutex::new(
        midi_out.create_virtual("rust-maschine-mk1-midi-out-virtual")?,
    ));

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
                Err(_err) => {
                    println!("control_writer; err: {:?}", _err)
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    });

    //
    // handle the midi for the dials
    //

    let midi_output_conn_for_send_dial = midi_output_conn.clone();

    let send_dial = move |dial: usize, value: u8| -> Result<()> {
        let mut midi_output_conn = midi_output_conn_for_send_dial
            .lock()
            .ok()
            .context("failed to lock mutex")?;

        assert!(dial < 128);

        let cc = CC_74;

        midi_output_conn
            .send(&[CC_PREFIX + CC_CHANNEL, cc, value])
            .context("send_pad: note off failed")?;

        Ok(())
    };

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
    for _ in 0..16 {
        angles.push(0.0);
    }

    let mut accumulators: Vec<f64> = vec![];
    for _ in 0..16 {
        accumulators.push(0.0);
    }

    let mut dials: Vec<u8> = vec![];
    for _ in 0..16 {
        dials.push(0);
    }

    let mut x0_history: Vec<f64> = vec![];
    let mut x1_history: Vec<f64> = vec![];
    let mut angle_history: Vec<f64> = vec![];

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

                        let mut any_changed = false;

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

                                let old_pressed = buttons[b];

                                if old_pressed != pressed {
                                    any_changed = true
                                }

                                buttons[b] = pressed;
                            }
                        }

                        //
                        // logging
                        //

                        if any_changed {
                            let mut line = String::new();
                            for (i, n) in buttons.iter().enumerate() {
                                let v = match n {
                                    true => 1,
                                    false => 0,
                                };

                                line.push_str(&format!("{i: >2}={v: <3}"));
                            }
                            println!("{}", line);
                        }

                        //
                        // sending MIDI
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

                        let mut any_changed = false;

                        let raw_dials = &raw_msg[1..23];

                        if raw_dials.ne(&last_raw_dials) {
                            // values are little-endian u16 (each 2-byte chunks is a pad state)
                            // first byte doesn't seem important
                            // last 10 bytes don't seem important
                            for (i, x) in raw_dials.chunks(2).enumerate() {
                                assert!(x.len() == 2);

                                let raw_n = u16::from_be_bytes([x[0], x[1]]);

                                let x0 = x[0] as f64;
                                let x1 = x[1] as f64;

                                let angle = x0.atan2(x1);
                                let last_angle = angles[i];
                                let mut delta = angle - last_angle;

                                if delta > std::f64::consts::PI {
                                    delta -= 2.0 * std::f64::consts::PI;
                                } else if delta < -std::f64::consts::PI {
                                    delta += 2.0 * std::f64::consts::PI;
                                }

                                angles[i] = angle;
                                accumulators[i] += delta;

                                let mut n = accumulators[i] as f64;

                                n /= 65535.0;
                                n *= 127.0;

                                assert!(n <= 127.0);

                                let n = n as u8;

                                let old_n = dials[i];

                                if n != old_n {
                                    any_changed = true;
                                }

                                dials[i] = n;

                                if i == 10 {
                                    x0_history.push(x0);
                                    if x0_history.len() > 256 {
                                        x0_history = x0_history
                                            [x0_history.len() - 256..x0_history.len()]
                                            .into();
                                    }

                                    x1_history.push(x1);
                                    if x1_history.len() > 256 {
                                        x1_history = x1_history
                                            [x1_history.len() - 256..x1_history.len()]
                                            .into();
                                    }

                                    angle_history.push(angle / 1.571 * 255.0);
                                    if angle_history.len() > 256 {
                                        angle_history = angle_history
                                            [angle_history.len() - 256..angle_history.len()]
                                            .into();
                                    }

                                    draw_chart(
                                        TextDrawingBackend(vec![PixelState::Empty; 10000])
                                            .into_drawing_area(),
                                        x0_history
                                            .iter()
                                            .enumerate()
                                            .map(|(i, x)| (i as f64, *x as f64))
                                            .collect(),
                                        x1_history
                                            .iter()
                                            .enumerate()
                                            .map(|(i, x)| (i as f64, *x as f64))
                                            .collect(),
                                        angle_history
                                            .iter()
                                            .enumerate()
                                            .map(|(i, x)| (i as f64, *x as f64))
                                            .collect(),
                                    )?;

                                    println!("{:?}", (x0, x1, raw_n, angle,));
                                }
                            }
                        }

                        last_raw_dials = raw_dials.into();

                        //
                        // logging
                        //

                        if any_changed {
                            let mut line = String::new();
                            for (i, n) in dials.iter().enumerate() {
                                line.push_str(&format!("{i: >2}:{n: <5}"));
                            }
                            // println!("{}", line);
                        }

                        //
                        // sending MIDI
                        //

                        for (i, n) in dials.iter().enumerate() {
                            if i == 10 {
                                let res = send_dial(i, *n);
                                if res.is_err() {
                                    println!("{:?}", res);
                                }
                            }
                        }
                    }
                }
                Err(_err) => {}
            }
        }
    });

    //
    // handle MIDI for pads
    //

    let midi_output_conn_for_send_pads = midi_output_conn.clone();

    let pad_is_on_by_note: Arc<Mutex<HashMap<u8, bool>>> = Arc::new(Mutex::new(HashMap::new()));

    let send_pad = move |note: u8, velocity: u8, requested_is_on: bool| -> Result<()> {
        let mut midi_output_conn = midi_output_conn_for_send_pads
            .lock()
            .ok()
            .context("failed to lock mutex")?;

        assert!(note < 128);

        let mut is_on = {
            let locked_pad_is_on_by_note = pad_is_on_by_note
                .lock()
                .ok()
                .context("send_pad: failed to lock pad_is_on_by_note")?;

            match locked_pad_is_on_by_note.get(&note) {
                Some(is_on) => *is_on,
                None => false,
            }
        };

        if requested_is_on {
            if is_on {
                midi_output_conn
                    .send(&[CHAN_1_POLYPHONIC_AFTERTOUCH, note, (velocity / 2)])
                    .context("send_pad: note aftertouch failed")?;

                return Ok(());
            };

            midi_output_conn
                .send(&[CHAN_1_NOTE_ON, note, velocity])
                .context("send_pad: note on failed")?;

            is_on = true;
        } else {
            if !is_on {
                return Ok(());
            };

            midi_output_conn
                .send(&[CHAN_1_NOTE_OFF, note, velocity])
                .context("send_pad: note off failed")?;

            is_on = false;
        }

        {
            let mut locked_pad_is_on_by_note = pad_is_on_by_note
                .lock()
                .ok()
                .context("failed to lock mutex")?;

            locked_pad_is_on_by_note.insert(note, is_on);
        };

        Ok(())
    };

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

                    let mut any_changed = false;

                    // values are little-endian u16 (each 2-byte chunks is a pad state)
                    for x in raw_msg.chunks(2) {
                        assert!(x.len() == 2);

                        let raw_n = u16::from_le_bytes([x[0], x[1]]);

                        // the top 4 bits is the pad number
                        let i = raw_n >> 12;

                        // the bottom 12 bits is the value
                        let mut n = (raw_n & (4096 - 1)) as f64;
                        n /= 4095.0; // normalize 
                        n *= 255.0; // re-scale to MIDI range

                        assert!(n <= 255.0);

                        let old_n = pads[i as usize];

                        if (n as u8) != old_n {
                            any_changed = true;
                        }

                        pads[i as usize] = n as u8;
                    }

                    //
                    // logging
                    //

                    if any_changed {
                        let mut line = String::new();
                        for (i, n) in pads.iter().enumerate() {
                            line.push_str(&format!("{i: >2}:{n: <5}"));
                        }
                        println!("{}", line);
                    }

                    //
                    // sending MIDI
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
