#!/bin/bash
dao="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && cd ../../.. && pwd )/dao"

$dao create 110 110 1 2
addr=$($dao addr | cut -d " " -f 4)
addr2=$(echo $addr | cut -c 2-)
addr3=${addr2::-1}
echo "address: $addr3"

$dao mint 1000000 $addr3

alice=$($dao keygen)
alice=$($dao keygen | cut -d " " -f 4)
alice2=$(echo $alice | cut -c 2-)
alice3=${alice2::-1}
echo "alice key: $alice3"

bob=$($dao keygen)
bob=$($dao keygen | cut -d " " -f 4)
bob2=$(echo $bob | cut -c 2-)
bob3=${bob2::-1}
echo "bob key: $bob3"

charlie=$($dao keygen)
charlie=$($dao keygen | cut -d " " -f 4)
charlie2=$(echo $charlie | cut -c 2-)
charlie3=${charlie2::-1}
echo "charlie key: $charlie3"

$dao airdrop $alice3 10000
$dao airdrop $bob3 100000
$dao airdrop $charlie3 10000

proposal=$($dao propose $alice3 $charlie3 10000 | cut -d " " -f 3)
proposal2=$(echo $proposal | cut -c 2-)
proposal3=${proposal2::-5}
echo "Proposal bulla: $proposal3"

$dao vote $alice3 yes
$dao vote $bob3 yes
$dao vote $charlie3 no

$dao exec $proposal3
