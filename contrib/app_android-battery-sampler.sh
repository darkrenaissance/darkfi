#!/bin/sh

# Use wireless debugging with this script
# Phone should be fully charged and unplugged

adb root

log_elapsed() {
    now=$(date +%s)
    elapsed=$(( now - start ))
    volt=$(adb shell cat /sys/class/power_supply/battery/voltage_now)
    echo $elapsed, $volt
}

start=$(date +%s)

for i in $(seq 1 20); do
    log_elapsed
    sleep 60
done

log_elapsed

