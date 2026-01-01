# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
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

import logging, time 
import datetime as dt
from collections import defaultdict as dd


class Model:

    def __init__(self):
        self.nodes = {}
        self.liliths = {}

    def add_node(self, node):
        channel_lookup = {}
        name = list(node.keys())[0]
        values = list(node.values())[0]
        info = values['result']
        channels = info['channels']
        
        self.nodes[name] = {}
        self.nodes[name]['outbound'] = {}
        self.nodes[name]['inbound'] = {}
        self.nodes[name]['manual'] = {}
        self.nodes[name]['direct'] = {}
        self.nodes[name]['direct_peer_discovery'] = None
        self.nodes[name]['event'] = {}
        self.nodes[name]['seed'] = {}
        self.nodes[name]['msgs'] = dd(list)

        for channel in channels:
            id = channel['id']
            channel_lookup[id] = channel

        for channel in channels:
            if channel['session'] != 'inbound':
                continue
            id = channel['id']
            url = channel_lookup[id]['url']
            self.nodes[name]['inbound'][f'{id}'] = url

        for i, id in enumerate(info['outbound_slots']):
            if id == 0:
                outbounds = self.nodes[name]['outbound'][f'{i}'] = ['none', 0]
                continue
            assert id in channel_lookup
            url = channel_lookup[id]['url']
            outbounds = self.nodes[name]['outbound'][f'{i}'] = [url, id]

        for channel in channels:
            if channel['session'] != 'seed':
                continue
            id = channel['id']
            url = channel['url']
            self.nodes[name]['seed'][f'{id}'] = url

        for channel in channels:
            if channel['session'] != 'manual':
                continue
            id = channel['id']
            url = channel['url']
            self.nodes[name]['manual'][f'{id}'] = url

        for channel in channels:
            if channel['session'] != 'direct':
                continue
            id = channel['id']
            url = channel['url']
            self.nodes[name]['direct'][f'{url}'] = [url, id]
    
    def add_offline(self, node, is_lilith: bool):
        name = list(node.keys())[0]
        values = list(node.values())[0]
        if is_lilith:
            self.liliths[name] = values
        else:
            self.nodes[name] = values

    def add_event(self, event):
        name = list(event.keys())[0]
        values = list(event.values())[0]
        params = values.get('params')
        event = params[0].get('event')
        info = params[0].get('info')

        t = time.localtime()
        current_time = time.strftime('%H:%M:%S', t)

        match event:                        
            case 'send':
                nano = info.get('time')
                cmd = info.get('cmd')
                chan = info.get('chan')
                addr = chan.get('addr')
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                msgs = self.nodes[name]['msgs']
                msgs[addr].append((t, event, cmd))
            case 'recv':
                nano = info.get('time')
                cmd = info.get('cmd')
                chan = info.get('chan')
                addr = chan.get('addr')
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                msgs = self.nodes[name]['msgs']
                msgs[addr].append((t, event, cmd))
            case 'inbound_connected':
                addr = info['addr']
                id = info.get('channel_id')
                self.nodes[name]['inbound'][f'{id}'] = addr
                logging.debug(f'{current_time}  inbound (connect):    {addr}')
            case 'inbound_disconnected':
                addr = info['addr']
                id = info.get('channel_id')
                self.nodes[name]['inbound'][f'{id}'] = {}
                logging.debug(f'{current_time}  inbound (disconnect): {addr}')
            case 'outbound_slot_sleeping':
                slot = info['slot']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{slot}')] = ['sleeping', 0]
                logging.debug(f'{current_time}  slot {slot}: sleeping')
            case 'outbound_slot_connecting':
                slot = info['slot']
                addr = info['addr']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{slot}')] = [f'connecting: addr={addr}', 0]
                logging.debug(f'{current_time}  slot {slot}: connecting   addr={addr}')
            case 'outbound_slot_connected':
                slot = info['slot']
                addr = info['addr']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{slot}')] = [f'connected: addr={addr}', 0]
                id = info['channel_id']
                self.nodes[name]['outbound'][f'{slot}'] = [addr, id]
                logging.debug(f'{current_time}  slot {slot}: connected    addr={addr}')
            case 'outbound_slot_disconnected':
                slot = info['slot']
                err = info['err']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{slot}')] = [f'disconnected: {err}', 0]
                logging.debug(f'{current_time}  slot {slot}: disconnected err={err}')
            case 'outbound_peer_discovery':
                attempt = info['attempt']
                state = info['state']
                event = self.nodes[name]['event']
                key = (f'{name}', 'outbound')
                event[key] = f'peer discovery: {state} (attempt {attempt})'
                logging.debug(f'{current_time}  peer_discovery: {state} (attempt {attempt})')
            case 'direct_connecting':
                connect_addr = info['connect_addr']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{connect_addr}')] = [f'connecting: addr={connect_addr}', 0]
                self.nodes[name]['direct'][f'{connect_addr}'] = ['', 0]
                logging.debug(f'{current_time}  direct (connecting):   addr={connect_addr}')
            case 'direct_connected':
                connect_addr = info['connect_addr']
                addr = info['addr']
                id = info['channel_id']
                event = self.nodes[name]['event']
                event[(f'{name}', f'{connect_addr}')] = [addr, id]
                self.nodes[name]['direct'][f'{connect_addr}'] = [addr, id]
                logging.debug(f'{current_time}  direct (connected):    addr={addr}')
            case 'direct_disconnected':
                connect_addr = info['connect_addr']
                err = info['err']
                self.nodes[name]['direct'][f'{connect_addr}'] = {}
                logging.debug(f'{current_time}  direct (disconnected): addr={connect_addr} err={err}')
            case 'direct_peer_discovery':
                if self.nodes[name]['direct_peer_discovery'] is None:
                    self.nodes[name]['direct_peer_discovery'] = 0
                attempt = info['attempt']
                state = info['state']
                event = self.nodes[name]['event']
                key = (f'{name}', 'direct')
                event[key] = f'peer discovery: {state} (attempt {attempt})'
                logging.debug(f'{current_time}  peer_discovery: {state} (attempt {attempt})')


    def add_lilith(self, lilith):
        key = list(lilith.keys())[0]
        values = list(lilith.values())[0]
        info = values['result']
        spawns = info['spawns']

        self.liliths[key] = {}
        self.liliths[key]['spawns'] = {}
        
        for (i, spawn) in enumerate(spawns):
            name = spawn['name']
            urls = spawn['urls']
            whitelist = spawn['whitelist']
            greylist = spawn['greylist']
            goldlist = spawn['goldlist']

            spawn = self.liliths[key]['spawns'][name] = {}
            spawn['urls'] = urls
            spawn['whitelist'] = whitelist
            spawn['greylist'] = greylist
            spawn['goldlist'] = goldlist

        #logging.debug(f'added lilith {self.liliths}')

    def __repr__(self):
        return f'{self.nodes}'
        return f'{self.liliths}'
