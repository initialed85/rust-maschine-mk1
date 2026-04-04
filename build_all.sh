#!/bin/bash

set -e

targets="aarch64-apple-darwin
x86_64-apple-darwin
aarch64-unknown-linux-gnu
x86_64-unknown-linux-gnu
aarch64-unknown-linux-musl
x86_64-unknown-linux-musl"

OS=$(uname -o) # Darwin
if [[ "${OS}" != "Darwin" ]]; then
	echo "error: full cross compilation will only work from a native Mac environment"
	exit 1
fi

cargo install cargo-zigbuild

PLATFORM=$(uname -p) # arm
if [[ "${PLATFORM}" == "arm" ]]; then
	echo "building for aarch64-apple-darwin"
	cargo build --release
	echo ""

	echo "building for x86_64-apple-darwin"
	cargo zigbuild --release --target "x86_64-apple-darwin"
	echo ""
else
	echo "building for x86_64-apple-darwin"
	cargo build --release
	echo ""

	echo "building for aarch64-apple-darwin"
	cargo zigbuild --release --target "aarch64-apple-darwin"
	echo ""
fi


echo "building for aarch64-unknown-linux-gnu"
docker build --platform=linux/aarch64 -f ./Dockerfile.gnu -t rust-maschine-mk1-build-aarch64-gnu .
docker run --platform=linux/aarch64 --rm -v "$(pwd):/srv/" --entrypoint cargo rust-maschine-mk1-build-aarch64-gnu build --release --target "aarch64-unknown-linux-gnu"
echo ""

echo "building for aarch64-unknown-linux-musl"
docker build --platform=linux/aarch64 -f ./Dockerfile.musl -t rust-maschine-mk1-build-aarch64-musl .
docker run --platform=linux/aarch64 --rm -v "$(pwd):/srv/" --entrypoint cargo rust-maschine-mk1-build-aarch64-musl build --release --target "aarch64-unknown-linux-musl"
echo ""

echo "building for x86_64-unknown-linux-gnu"
docker build --platform=linux/amd64 -f ./Dockerfile.gnu -t rust-maschine-mk1-build-x86_64-gnu .
docker run --platform=linux/amd64 --rm -v "$(pwd):/srv/" --entrypoint cargo rust-maschine-mk1-build-x86_64-gnu build --release --target "x86_64-unknown-linux-gnu"
echo ""

echo "building for x86_64-unknown-linux-musl"
docker build --platform=linux/amd64 -f ./Dockerfile.musl -t rust-maschine-mk1-build-x86_64-musl .
docker run --platform=linux/amd64 --rm -v "$(pwd):/srv/" --entrypoint cargo rust-maschine-mk1-build-x86_64-musl build --release --target "x86_64-unknown-linux-musl"
echo ""

mkdir -p ./dist
rm -fr ./dist/*
cp -fv target/aarch64-apple-darwin/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-aarch64-apple-darwin
cp -fv target/x86_64-apple-darwin/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-x86_64-apple-darwin
cp -fv target/aarch64-unknown-linux-gnu/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-aarch64-unknown-linux-gnu
cp -fv target/aarch64-unknown-linux-musl/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-aarch64-unknown-linux-musl
cp -fv target/x86_64-unknown-linux-gnu/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-x86_64-unknown-linux-gnu
cp -fv target/x86_64-unknown-linux-musl/release/rust_maschine_mk1 ./dist/rust_maschine_mk1-x86_64-unknown-linux-musl
