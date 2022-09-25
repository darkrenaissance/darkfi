#!/bin/bash
cargo run create 110 110 1 2
addr=$(cargo run addr | cut -d " " -f 4)
addr2=$(echo $addr | cut -c 2-)
addr3=${addr2::-1}
echo $addr3

cargo run mint 1000000 $addr3

alice=$(cargo run keygen alice)
alice=$(cargo run keygen alice | cut -d " " -f 4)
alice2=$(echo $alice | cut -c 2-)
alice3=${alice2::-1}
echo $alice3

bob=$(cargo run keygen bob)
bob=$(cargo run keygen bob | cut -d " " -f 4)
bob2=$(echo $bob | cut -c 2-)
bob3=${bob2::-1}
echo $bob3

charlie=$(cargo run keygen charlie)
charlie=$(cargo run keygen charlie | cut -d " " -f 4)
charlie2=$(echo $charlie | cut -c 2-)
charlie3=${charlie2::-1}
echo $charlie3

cargo run airdrop alice 10000
cargo run airdrop bob 100000
cargo run airdrop charlie 10000

proposal=$(cargo run propose alice $charlie3 10000 | cut -d " " -f 3)
proposal2=$(echo $proposal | cut -c 2-)
proposal3=${proposal2::-1}
echo $proposal3

cargo run vote alice yes
cargo run vote bob yes
cargo run vote charlie no

cargo run exec $proposal3

