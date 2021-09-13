#!/bin/bash


arg=$1
gatewayd_path="./target/release/gatewayd"
cashierd_path="./target/release/cashierd"
darkfid_path="./target/release/darkfid"
drk_path="./target/release/drk"

echo Running demo...

if [ "$arg" = "-d" ]; then
	echo DEBUG MODE	
	gatewayd_path="./target/debug/gatewayd"
	cashierd_path="./target/debug/cashierd"
	darkfid_path="./target/debug/darkfid"
	drk_path="./target/debug/drk"
fi

echo Open new tab and run: 
echo "	$ " $drk_path --help

trap cleanup SIGINT 

function cleanup()
{
	screen -X -S "gatewayd" quit
	screen -X -S "cashierd" quit
	screen -X -S "darkfid" quit
	echo Exit demo...
}

# Start gateway daemon 
screen -S "gatewayd" -dm "$gatewayd_path" -v 
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to start gateway daemon: $status"
  exit $status
fi

# Start cashier daemon 
screen -S "cashierd" -dm "$cashierd_path" -v
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to start cashier daemon: $status"
  exit $status
fi

# Start darkfi daemon 
screen -S "darkfid" -dm "$darkfid_path" -v
status=$?
if [ $status -ne 0 ]; then
  echo "Failed to start darkfi daemon: $status"
  exit $status
fi

# Exit with an error if it detects that either of the processes has exited.
# Otherwise it loops forever, waking up every 60 seconds 
while sleep 60; do
  ps aux |grep "gatewayd" |grep -q -v grep
  PROCESS_1_STATUS=$?
  ps aux |grep "cashierd" |grep -q -v grep
  PROCESS_2_STATUS=$?
  ps aux |grep "darkfid" |grep -q -v grep
  PROCESS_3_STATUS=$?

  if [ $PROCESS_1_STATUS -ne 0 -o $PROCESS_2_STATUS -ne 0 -o $PROCESS_3_STATUS -ne 0 ]; then
    echo "One of the processes has already exited."
	cleanup
    exit 1
  fi
done


