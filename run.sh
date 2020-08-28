set -euxo pipefail

# This relies on a version of cross that can read `context` and `dockerfile`
# from Cross.toml. You can install it with:
#
#     cargo install --git=https://github.com/alsuren/cross --branch=docker-build-context
#
TARGET=${TARGET:-armv7-unknown-linux-gnueabi}
TARGET_SSH=${TARGET_SSH:-pi@raspberrypi.local}
PROFILE=${PROFILE:-debug}
RUN=${RUN:-1}

if [ $PROFILE = release ]
then
    time cross build --target $TARGET --release
elif [ $PROFILE = debug ]
then
    time cross build --target $TARGET
else
    echo "Invalid profile '$PROFILE'"
    exit 1
fi

time rsync --progress target/$TARGET/$PROFILE/read-all-devices $TARGET_SSH:read-all-devices-next
time rsync --progress target/$TARGET/$PROFILE/publish-mqtt $TARGET_SSH:publish-mqtt-next
ssh $TARGET_SSH sudo setcap 'cap_net_raw,cap_net_admin+eip' ./read-all-devices-next
ssh $TARGET_SSH sudo setcap 'cap_net_raw,cap_net_admin+eip' ./publish-mqtt-next

if [ $RUN -eq 1 ]
then
    # ssh $TARGET_SSH sudo systemctl stop publish-mqtt.service
    ssh $TARGET_SSH env RUST_BACKTRACE=1 ./publish-mqtt-next
fi
