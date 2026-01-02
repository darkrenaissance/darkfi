#!/usr/bin/env python3
import uuid
import hashlib
from datetime import datetime
from sys import argv

from prettytable import PrettyTable, HEADER

EVENTS = [
    {
        "start": "20230526T140000Z",
        "end": "20230526T160000Z",
        "track": "Math",
        "topic": "Elliptic Curves",
        "title": "Introduction to Elliptic Curves",
        "#": 1,
        "recording": "",
    },

    {
        "start": "20230530T140000Z",
        "end": "20230530T160000Z",
        "track": "Math",
        "topic": "Abstract Algebra",
        "title": "Group Structure and Homomorphisms",
        "#": 1,
        "recording": "https://ipfs.io/ipfs/QmRNgGSHjJNSXCnXBF65ThWSSWPyamJi6giBA26uVJrU1W",
    },

    {
        "start": "20230615T140000Z",
        "end": "20230615T160000Z",
        "track": "Research",
        "topic": "Consensus",
        "title": "DarkFi Consensus Algorithm and Control Theory",
        "#": 1,
        "recording": "https://ipfs.io/ipfs/QmQoe1LmfL1ubuML4LQy6jdeDjPKEGD7Z2Dk9kEevqBDtw",
    },

    {
        "start": "20230622T140000Z",
        "end": "20230622T160000Z",
        "track": "Research",
        "topic": "Consensus",
        "title": "DarkFi Consensus Algorithm and Control Theory",
        "#": 2,
        "recording": "",
    },

    {
        "start": "20230727T140000Z",
        "end": "20230727T160000Z",
        "track": "Dev",
        "topic": "Event Graph",
        "title": "Walkthrough the Event Graph",
        "#": 1,
        "recording": "",
    },
]

def print_table():
    x = PrettyTable()
    x.field_names = ["Date", "Track", "Topic", "#", "Title", "Rec"]
    x.align = "l"
    x.hrules = HEADER
    x.junction_char = "|"

    for event in EVENTS:
        timestamp = event["start"]
        parsed = datetime.strptime(timestamp, "%Y%m%dT%H%M%SZ")
        formatted = parsed.strftime("%a %d %b %Y %H:%M UTC")

        s = ''.join(ch if ch.isalnum() else '' for ch in event["title"])
        ics_file = f"{event['start']}_{s}.ics"

        if event["recording"] != "":
            rec = f"[dl]({event['recording']})"
        else:
            rec = "n/a"
    
        x.add_row([
            f"[{formatted}]({ics_file})",
            event["track"],
            event["topic"],
            event["#"],
            event["title"],
            rec,
        ])

    print("# Developer Seminars\n")
    print("Weekly seminars on DarkFi, cryptography, code and other topics.")
    print("Each seminar is usually 2 hours long\n")
    print(x)
    print("\nThe link for calls is")
    print("[meet.jit.si/darkfi-seminar](https://meet.jit.si/darkfi-seminar).")
    print("\nFor the math seminars, we use a collaborative whiteboard called")
    print("[therapy](https://github.com/narodnik/therapy) that we made.")
    print("The canvas will also be shared on Jitsi calls.\n")
    print("Videos will be uploaded online and linked here.")
    print("Join [our chat](https://dark.fi/book/misc/darkirc/darkirc.html)")
    print("for more info. Links and text chat will happen there during the calls.")


def print_ics():
    for event in EVENTS:
        ics = []
        ics.append("BEGIN:VCALENDAR")
        ics.append("VERSION:2.0")
        ics.append("PRODID:-//dark.fi//Seminars//EN")
        ics.append("BEGIN:VEVENT")
        ics.append(f"SUMMARY:DarkFi Seminar: {event['title']}")
        m = hashlib.md5()
        m.update((event["start"] + event["title"]).encode("utf-8"))
        ics.append(f"UID:{uuid.UUID(m.hexdigest())}")
        ics.append(f"DTSTART:{event['start']}")
        ics.append(f"DTEND:{event['end']}")
        ics.append(f"DTSTAMP:{event['start']}")
        ics.append(f"CATEGORIES:{event['topic']}")
        ics.append("URL:https://meet.jit.si/darkfi-seminar")
        ics.append("END:VEVENT")
        ics.append("END:VCALENDAR")

        s = ''.join(ch if ch.isalnum() else '' for ch in event["title"])
        ics_file = f"{event['start']}_{s}.ics"

        with open(f"book/dev/{ics_file}", "w") as f:
            f.write('\n'.join(ics))
            f.write('\n')



def usage():
    print("usage: Use --ics or --table as a flag")
    exit(1)

if __name__ == "__main__":
    if len(argv) != 2:
        usage()

    if argv[1] == "--ics":
        print_ics()
        exit(0)

    if argv[1] == "--table":
        print_table()
        exit(0)

    usage()
