#!/bin/sh

# Use wireless debugging with this script
# Phone should be fully charged and unplugged

# adb root
# Long hold wireless debugging in developer options
# Select "Pair device with pairing code"
# adb pair ipaddr:port
# Now use the ipaddr:port on the wireless debugging screen
# adb connect ipaddr:port

log_elapsed() {
    now=$(date +%s)
    elapsed=$(( now - start ))
    #val=$(adb shell cat /sys/class/power_supply/battery/voltage_now)
    val=$(adb shell dumpsys battery | grep level | awk '{print $2}')
    echo $elapsed, $val
}

start=$(date +%s)

for i in $(seq 1 200); do
    log_elapsed
    if [ $? -ne 0 ]; then
        break
    fi
    sleep 60
done

log_elapsed

