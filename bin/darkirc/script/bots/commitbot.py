# -*- coding: utf-8 -*-

from http.server import BaseHTTPRequestHandler,HTTPServer
import json
import sys
import irc

# Attributes of the server this bot will run on
SERVER_HOST = 'server.url.or.ip'
SERVER_PORT = 11022

# Attributes of the IRC connection
IRC_SERVER = '127.0.0.1'
IRC_PORT = 6667
IRC_CHANNEL = ['#dev']
IRC_NICK = 'commits-notifier'

# Set the password for your registered empty, leave empty if not applicable
# Note: freenode(and potentially other servers) want password to be of the form
# "nick:pass", so for ex. IRC_PASS = 'WfTestBot:mypass123'
IRC_PASS = ''

# a dictionary of branches push-related events should be enabled for, or empty if all are enabled
GH_PUSH_ENABLED_BRANCHES = [] # for example, ['master', 'testing', 'author/repo:branch']

# a dictionary of branches push-related events should be ignored for, or empty if all are enabled
GH_PUSH_IGNORE_BRANCHES = ['gh-pages']

# a list of push-related events the bot should post notifications for
GH_PUSH_ENABLED_EVENTS = ['push'] # no others supported for now

# a list of PR-related events the bot should post notifications for
# notice 'merged' is just a special case of 'closed'
GH_PR_ENABLED_EVENTS = ['opened', 'closed', 'reopened'] # could also add 'synchronized', 'labeled', etc.

# handle POST events from github server
# We should also make sure to ignore requests from the IRC, which can clutter
# the output with errors
CONTENT_TYPE = 'content-type'
CONTENT_LEN = 'content-length'
EVENT_TYPE = 'x-github-event'

ircc = irc.IRC()
ircc.connect(IRC_SERVER, IRC_PORT, IRC_CHANNEL, IRC_NICK)

def handle_push_event(irc, data):
    if GH_PUSH_ENABLED_BRANCHES:
        branch = get_branch_name_from_push_event(data)
        repo = data['repository']['full_name']
        repobranch = repo + ':' + branch
        if not branch in GH_PUSH_ENABLED_BRANCHES:
            if not repobranch in GH_PUSH_ENABLED_BRANCHES:
                return
    
    if GH_PUSH_IGNORE_BRANCHES:
        branch = get_branch_name_from_push_event(data)
        if branch in GH_PUSH_IGNORE_BRANCHES:
            return

    if 'push' in GH_PUSH_ENABLED_EVENTS:
        handle_forward_push(irc, data)

def handle_pull_request(irc, data):
    author = data['sender']['login']
    if not data['action'] in GH_PR_ENABLED_EVENTS:
        return

    action = data['action']
    merged = data['pull_request']['merged']
    action = 'merged' if action == 'closed' and merged else action
    pr_num = '#' + str(data['number'])
    title = data['pull_request']['title']

    print("PR event:")
    print(f"@{author} {action} pull request {pr_num}: {title}")
    print("==============================================")

    irc.send("#dev", f"@{author} {action} pull request {pr_num}: {title}")

def get_branch_name_from_push_event(data):
    return data['ref'].split('/')[-1]

def handle_forward_push(irc, data):
    author = data['commits'][0]['author']['name']

    num_commits = len(data['commits'])
    num_commits = str(num_commits) + " commit" + ('s' if num_commits > 1 else '')

    branch = get_branch_name_from_push_event(data)

    commits = list(map(fmt_commit, data['commits']))
    for commit in reversed(commits):
        print("Push event:")
        print(f"@{author} pushed {num_commits} to {branch}: {commit}")
        print("==============================================")
        irc.send("#dev", f"@{author} pushed {num_commits} to {branch}: {commit}")

def fmt_commit(cmt):
    hsh = cmt['id'][:10]
    # author = cmt['author']['name']
    message = cmt['message'].split("\n")
    message = message[0] \
            + ('...' if len(message) > 1 else '')

    return '{}: {}'.format(hsh, message)

class MyHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        pass
    def do_CONNECT(self):
        pass
    def do_POST(self):
        if not all(x in self.headers for x in [CONTENT_TYPE, CONTENT_LEN, EVENT_TYPE]):
            return
        content_type = self.headers['content-type']
        content_len = int(self.headers['content-length'])
        event_type = self.headers['x-github-event']

        if content_type != "application/json":
            self.send_error(400, "Bad Request", "Expected a JSON request")
            return

        data = self.rfile.read(content_len)
        if sys.version_info < (3, 6):
            data = data.decode()

        self.send_response(200)
        self.send_header('content-type', 'text/html')
        self.end_headers()
        self.wfile.write(bytes('OK', 'utf-8'))

        if event_type == 'push':
            handle_push_event(ircc, json.loads(data))
        elif event_type == 'pull_request':
            handle_pull_request(ircc, json.loads(data))
        return

# Run Github webhook handling server
try:
    server = HTTPServer((SERVER_HOST, SERVER_PORT), MyHandler)
    server.serve_forever()
except KeyboardInterrupt:
    print("Exiting")
    server.socket.close()
