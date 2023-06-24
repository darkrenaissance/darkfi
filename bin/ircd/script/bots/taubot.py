import argparse
import irc
import json
import socket
import sys

# parse arguments
parser = argparse.ArgumentParser(description='IRC bot to send a pipe to an IRC channel')
parser.add_argument('--server',default='127.0.0.1', help='IRC server')
parser.add_argument('--port', default=11066, help='port of the IRC server')
parser.add_argument('--nickname', help='bot nickname in IRC')
parser.add_argument('--channel', default="#dev", action='append', help='channel to join')
parser.add_argument('--pipe', default="/tmp/tau_pipe" , help='pipe to read from')
parser.add_argument('--skip', default="prv", help='Project or Tags to skip notifications for')
parser.add_argument('--alt-chan', default="#test", required='--skip' in sys.argv, help='Alternative channel to send notifications to when there are skipped tasks')

args = parser.parse_args()

channels = [args.channel, args.alt_chan] if args.alt_chan is not None else args.channel

ircc = irc.IRC()
ircc.connect(args.server,int(args.port), channels, args.nickname)

while True:
    with open(args.pipe) as handle:
        while True:
            log_line = handle.readline()
            if not log_line:
                break
            print(log_line)
            print("======================================")
            task = json.loads(log_line)
            channel = args.channel

            for event in task['events']:
                cmd = event['action']
                if cmd == "add_task":
                    user = task['owner']
                    id = task['id']
                    title = task['title']
                    assigned = ", ".join(task['assign'])

                    project = task['project'] if task['project'] is not None else []
                    if args.skip in project or args.skip in task['tags']:
                        channel = args.alt_chan

                    if len(assigned) > 0:
                        notification = f"{user} added task ({id}): {title}. assigned to {assigned}"
                    else:
                        notification = f"{user} added task ({id}): {title}"
                    # print(notification)
                    ircc.send(channel, notification)
                elif cmd == "state":
                    user = event['author']
                    state = event['content']
                    id = task['id']
                    title = task['title']

                    project = task['project'] if task['project'] is not None else []
                    if args.skip in project or args.skip in task['tags']:
                        channel = args.alt_chan

                    if state == "start":
                        notification = f"{user} started task ({id}): {title}"
                    elif state == "pause":
                        notification = f"{user} paused task ({id}): {title}"
                    elif state == "stop":
                        notification = f"{user} stopped task ({id}): {title}"
                    elif state == "cancel":
                        notification = f"{user} canceled task ({id}): {title}"
                    # print(notification)
                    ircc.send(channel, notification)
                elif cmd == "comment":
                    user = event['author']
                    id = task['id']
                    title = task['title']
                    
                    project = task['project'] if task['project'] is not None else []
                    if args.skip in project or args.skip in task['tags']:
                        channel = args.alt_chan

                    notification = f"{user} commented on task ({id}): {title}"
                    # print(notification)
                    ircc.send(channel, notification)
                elif cmd == "assign":
                    user = event['author']
                    assignees = event['content']
                    id = task['id']
                    title = task['title']

                    project = task['project'] if task['project'] is not None else []
                    if args.skip in project or args.skip in task['tags']:
                        channel = args.alt_chan

                    notification = f"{user} reassigned task ({id}): {title} to {assignees}"
                    # print(notification)
                    ircc.send(channel, notification)
