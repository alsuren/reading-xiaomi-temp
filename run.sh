set -euxo pipefail

time cross build --target armv7-unknown-linux-gnueabihf --release
time rsync target/armv7-unknown-linux-gnueabihf/release/xiaomi pi@raspberrypi.local:xiaomi
time ssh pi@raspberrypi.local ./xiaomi
