# rust-maschine-mk1

# status: very early days

Some hacking to get an old Maschine Mk1 that @lowaa gave me working on contemporary OSes- at least
works on macOS at the moment but because it's using [boddlnagg/midir](https://github.com/boddlnagg/midir)
under the hood, in theory it should work on Linux as well (and maybe Windows? I dunno).

## Tasks

- Pads
  - [DONE] Just make them spit out A minor from A0
  - [DONE] Make them do polyphonic aftertouch
  - [TODO] Make them configurable
  - [TODO] Something with the LED backlights
- Dials
  - [TODO] Make them do something (MIDI CC messages?)
- Buttons
  - [TODO] Make them do something (MIDI CC messages?)
- Displays
  - [TODO] Make them do literally anything
  - [TODO] Some kinda TUI abstraction?

## Usage

- Plug in your Maschine Mk1
- Build + run the code
- MIDI output named `rust-maschine-mk1-midi-out-virtual` should be created
  - I've just been using [Bespoke](https://www.bespokesynth.com/) but given it's a MIDI device, it should work with anything

You can build and run the code like this

```shell
cargo build && target/debug/rust_maschine_mk1
```
