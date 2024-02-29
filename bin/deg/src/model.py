# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2024 Dyne.org foundation
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
    
    def add_eg(self, node):
        name = list(node.keys())[0]
        values = list(node.values())[0]
        info = values['result']['eventgraph_info']

        self.nodes[name] = {}
        self.nodes[name]['current_genesis'] = {}
        self.nodes[name]['broadcasted_ids'] = {}
        self.nodes[name]['synced'] = {}
        self.nodes[name]['event'] = {}
        self.nodes[name]['unreferenced_tips'] = {}
        self.nodes[name]['msgs'] = dd(list)

        if info['current_genesis']:
            self.nodes[name]['current_genesis'] = info['current_genesis']

        if info['broadcasted_ids']:
            self.nodes[name]['broadcasted_ids'] = info['broadcasted_ids']

        if info['synced']:
            self.nodes[name]['synced'] = info['synced']

        if info['unreferenced_tips']:
            self.nodes[name]['unreferenced_tips'] = info['unreferenced_tips']
    
    def add_offline(self, node):
        name = list(node.keys())[0]
        values = list(node.values())[0]
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
                ev_info = info.get('info')
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                msgs = self.nodes[name]['msgs']
                msgs[name].append((t, "send", cmd, ev_info))
            case 'recv':
                nano = info.get('time')
                cmd = info.get('cmd')
                ev_info = info.get('info')
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                msgs = self.nodes[name]['msgs']
                msgs[name].append((t, "recv", cmd, ev_info))

    def __repr__(self):
        return f'{self.nodes}'
