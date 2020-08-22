set -euxo pipefail

# This relies on a version of cross that can read `context` and `dockerfile`
# from Cross.toml. You can install it with:
#
#     cargo install --git=https://github.com/alsuren/cross --branch=docker-build-context
#
TARGET_SSH=${TARGET_SSH:-pi@raspberrypi.local}
PROFILE=${PROFILE:-debug}
RUN=${RUN:-1}

if [ $PROFILE = release ]
then
    time cross build --target armv7-unknown-linux-gnueabihf --release
elif [ $PROFILE = debug ]
then
    time cross build --target armv7-unknown-linux-gnueabihf
else
    echo "Invalid profile '$PROFILE'"
    exit 1
fi

time rsync target/armv7-unknown-linux-gnueabihf/$PROFILE/read-all-devices $TARGET_SSH:read-all-devices
# time rsync target/armv7-unknown-linux-gnueabihf/$PROFILE/publish-mqtt $TARGET_SSH:publish-mqtt
ssh $TARGET_SSH sudo setcap 'cap_net_raw,cap_net_admin+eip' ./read-all-devices
if [ $RUN -eq 1 ]
then
    ssh $TARGET_SSH ./read-all-devices
fi
