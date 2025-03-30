#!/usr/bin/env python

# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

import asyncio, json, random, sys, time, argparse, csv, signal, statistics
from collections import defaultdict

class JsonRpc:

    async def start(self, server, port):
        reader, writer = await asyncio.open_connection(server, port)
        self.reader = reader
        self.writer = writer

    async def stop(self):
        self.writer.close()
        await self.writer.wait_closed()

    async def _make_request(self, method, params):
        ident = random.randint(0, 2**16)
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": ident,
        }

        message = json.dumps(request) + "\n"
        self.writer.write(message.encode())
        await self.writer.drain()

        data = await self.reader.readline()
        message = data.decode().strip()
        response = json.loads(message)
        return response

    async def _subscribe(self, method, params):
        ident = random.randint(0, 2**16)
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": ident,
        }

        message = json.dumps(request) + "\n"
        self.writer.write(message.encode())
        await self.writer.drain()

    async def ping(self):
        return await self._make_request("ping", [])

    async def dnet_switch(self, state):
        return await self._make_request("dnet.switch", [state])

    async def dnet_subscribe_events(self):
        return await self._subscribe("dnet.subscribe_events", [])

async def collect_messages(server, port, output_file):
    rpc = JsonRpc()
    while True:
        try:
            await rpc.start(server, port)
            break
        except OSError:
            pass

    file = open(output_file, "a", encoding="utf-8")
    print(f"Started collecting measurements, saving to {output_file} ...")
    try:
        await rpc.dnet_switch(True)
        await rpc.dnet_subscribe_events()

        file_writer = csv.writer(file, delimiter="\t")
        count = 0
        while True:
            data = await rpc.reader.readline()
            data = json.loads(data)

            params = data["params"][0]
            ev = params["event"]
            if ev != "recv":
                continue

            row = [params["info"]["cmd"], int(params["info"]["time"]), params["info"]["chan"]["addr"]]
            file_writer.writerow(row)
            count += 1
            print(f"Messages collected: {count}", end="\r", flush=True)
    except asyncio.CancelledError:
        print("Stopping message collection.")
    finally:
        await rpc.dnet_switch(False)
        await rpc.stop()
        file.close()

def analyze_messages(output_file):
    data = []
    with open(output_file, mode="r", encoding="utf-8") as file:
        tsv_reader = csv.reader(file, delimiter="\t")
        # 0 - message_type
        # 1 - time in nano seconds
        # 2 - peer_addr
        for row in tsv_reader:
            row[1] = int(row[1])
            data.append(row)

    print(f"Analyzing the collected measurement data from {output_file} ...")
    # Group by message_type and peer_addr since we want to get the number of
    # messages we received from a particular peer in some time window
    grouped_data = defaultdict(list)
    for item in data:
        grouped_data[(item[0], item[2])].append(item)

    # Use a 10 second window size in nano seconds
    window_size = 10 * 1_000_000_000
    results = defaultdict(list)

    for key, messages in grouped_data.items():
        messages.sort(key=lambda x: x[1]) # Sort by time

        if len(messages) < 2:
            continue

        # We will start with the first item and count the number of messages
        # of some particular message_type in a 10 second window for each peer
        start_time = messages[0][1]
        window_counts = []
        count = 0

        for message in messages:
            message_time = message[1]
            if message_time < start_time + window_size:
                count += 1
            else:
                window_counts.append(count)
                start_time = message_time
                count = 1

        if count:
            window_counts.append(count)

        message_type = key[0]
        # Store counts of the same message_type across different peers
        results[message_type].extend(window_counts)

    for message_type, counts in results.items():
        print(f"Message Type: {message_type}")
        print(f"    Count: {len(counts)}")
        print(f"    Mean : {statistics.mean(counts)}")
        print(f"    Median: {statistics.median(counts)}")
        print(f"    Variance: {statistics.variance(counts)}")
        print(f"    Max: {max(counts)}")
        print(f"    Min: {min(counts)}\n")

async def main(argv):
    parser = argparse.ArgumentParser(description='Tool to collect and analyze measurement of received messages')
    parser.add_argument('--server', default='127.0.0.1', help='RPC server')
    parser.add_argument('--port', default=26660, help='Port of the RPC server')
    parser.add_argument('--output-file', default='/tmp/node-metering-data.tsv', help='Location of the file containing the collected data')
    parser.add_argument('--analyze', action='store_true', help='Analyzes existing message from the output file without collecting new ones')

    args = parser.parse_args()
    if args.analyze:
        analyze_messages(args.output_file)
    else:
        collect_task = asyncio.create_task(collect_messages(args.server, args.port, args.output_file))
        loop = asyncio.get_event_loop()
        loop.add_signal_handler(signal.SIGINT, lambda: collect_task.cancel())
        await collect_task


asyncio.run(main(sys.argv))
