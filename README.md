# rust-maschine-mk1

# status: working for pads and dials

Some hacking to get an old Maschine Mk1 that [@lowaa](https://github.com/lowaa) gave me working on contemporary OSes- at least
works on macOS at the moment but because it's using [boddlnagg/midir](https://github.com/boddlnagg/midir)
under the hood, in theory it should work on Linux as well (and maybe Windows? I dunno).

Props to [shaduzlabs/cabl](https://github.com/shaduzlabs/cabl) and [quasart/cabl](https://github.com/quasart/cabl) for having
done the hard part of the reverse engineering already.

## Tasks

- Pads
  - [DONE] Read / decode them
  - [DONE] Make the send A minor from A0
  - [DONE] Make them do polyphonic aftertouch
  - [TODO] Make them configurable
  - [TODO] Something with the LED backlights
- Dials
  - [DONE] Read / decode them
  - [DONE] Make them send MIDI CC messages
- Buttons
  - [DONE] Read / decode them
  - [TODO] Make them send MIDI CC messages
  - [TODO] Something with the LED backlights
- Displays
  - [TODO] Make them do literally anything
  - [TODO] Some kinda TUI abstraction?
- Overall
  - [TODO] Change to some kind message-passing / channel-driven approach
  - [TODO] Split into modules
  - [TODO] Add some tests
  - [TODO] Retry loops (e.g. avoid needing to restart on device reconnection)
- Stretch DevEx
  - [TODO] Split device input and MIDI output into different subprocesses w/ RPC to maintain MIDI connection during hot reloads

## Pre-built binaries

Pre-built binaries can be found at the following URLs:

- [public.initialed85.cc/rust_maschine_mk1-aarch64-apple-darwin](https://public.initialed85.cc/rust_maschine_mk1-aarch64-apple-darwin)
- [public.initialed85.cc/rust_maschine_mk1-x86_64-apple-darwin](https://public.initialed85.cc/rust_maschine_mk1-x86_64-apple-darwin)
- [public.initialed85.cc/rust_maschine_mk1-aarch64-unknown-linux-gnu](https://public.initialed85.cc/rust_maschine_mk1-aarch64-unknown-linux-gnu)
- [public.initialed85.cc/rust_maschine_mk1-x86_64-unknown-linux-gnu](https://public.initialed85.cc/rust_maschine_mk1-x86_64-unknown-linux-gnu)
- [public.initialed85.cc/rust_maschine_mk1-aarch64-unknown-linux-musl](https://public.initialed85.cc/rust_maschine_mk1-aarch64-unknown-linux-musl)
- [public.initialed85.cc/rust_maschine_mk1-x86_64-unknown-linux-musl](https://public.initialed85.cc/rust_maschine_mk1-x86_64-unknown-linux-musl)

## Building and running

```shell
cargo build && target/debug/rust_maschine_mk1
```

## Usage

- Plug in your Maschine Mk1
- Build + run the code
- MIDI output named `rust-maschine-mk1-midi-out-virtual` should be created
  - I've just been using [Bespoke](https://www.bespokesynth.com/) but given it's a MIDI device, it should work with anything

You can build and run the code like this

## Iterating

I normally run two shells:

```shell
# shell 1
find Cargo.toml src | entr -n -r -cc -s "sleep 1 ; cargo build"

# shell 2
find target/debug/rust_maschine_mk1 | entr -n -r -cc -s "time RUST_LOG=trace target/debug/rust_maschine_mk1; echo ''; date"
```

Then I'll have Bespoke Synth running to message around with the controller fed into an instrument or VST or something.
