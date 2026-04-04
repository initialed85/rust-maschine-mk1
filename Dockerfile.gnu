FROM rust:trixie

WORKDIR /srv/

RUN rustup target add aarch64-unknown-linux-gnu
RUN rustup target add x86_64-unknown-linux-gnu

RUN apt-get update && apt-get install -y libasound2 libasound2-dev alsa-utils
RUN ldconfig
