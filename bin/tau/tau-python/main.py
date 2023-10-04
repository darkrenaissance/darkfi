#!/usr/bin/python3
import asyncio, json, os, sys, tempfile
from datetime import datetime
from tabulate import tabulate
from colorama import Fore, Back, Style

import api, lib.util

# USERNAME = lib.config.get("username", "Anonymous")
USERNAME = "Anonymous"

async def add_task(task_args):
    task = {
        "title": None,
        "tags": [],
        "desc": None,
        "assign": [],
        "project": [],
        "due": None,
        "rank": None,
        "created_at": lib.util.now(),
        "state": "open"
    }
    # Everything that isn't an attribute is part of the title
    # Open text editor if desc isn't set to write desc text
    title_words = []
    for arg in task_args:
        if arg[0] == "+":
            tag = arg[1:]
            if tag in task["tags"]:
                print(f"error: duplicate tag +{tag} in task", file=sys.stderr)
                sys.exit(-1)
            task["tags"].append(tag)
        elif arg[0] == "@":
            assign = arg[1:]
            if assign in task["assign"]:
                print(f"error: duplicate assign @{assign} in task", file=sys.stderr)
                sys.exit(-1)
            task["assign"].append(assign)
        elif ":" in arg:
            attr, val = arg.split(":", 1)
            set_task_attr(task, attr, val)
        else:
            title_words.append(arg)

    title = " ".join(title_words)
    if len(title) == 0:
        print("Error: Title is required")
        exit(-1)
    task["title"] = title
    if task["desc"] is None:
        task["desc"] = prompt_description_text(task)
    
    if task["desc"].strip() == '':
        print("Abort adding the task due to empty description.")
        exit(-1)

    id = await api.add_task(task)
    print(f"Created task {id}.")

def prompt_text(comment_lines):
    temp = tempfile.NamedTemporaryFile()
    temp.write(b"\n")
    for line in comment_lines:
        temp.write(line.encode() + b"\n")
    temp.flush()
    editor = os.environ.get('EDITOR') if os.environ.get('EDITOR') else 'nano'    
    os.system(f"{editor} {temp.name}")
    desc = open(temp.name, "r").read()
    # Remove comments and empty lines from desc
    cleaned = []
    for line in desc.split("\n"):
        if line == "# ------------------------ >8 ------------------------":
            break
        if line.startswith("#"):
            continue
        cleaned.append(line)

    return "\n".join(cleaned)

def prompt_description_text(task):
    return prompt_text([
        "# Write task description above this line.",
        "# These lines will be removed.",
        "# An empty description aborts adding the task",
        "\n# ------------------------ >8 ------------------------",
        "# Do not modify or remove the line above.",
        "# Everything below it will be ignored.",
        f"\n{tabulate_task(task)}"
    ])

def prompt_comment_text():
    return prompt_text([
        "# Write comments above this line",
        "# These lines will be removed"
    ])

def set_task_attr(task, attr, val):
    # templ = lib.util.task_template
    assert attr in ["desc", "rank", "due", "project"]
    print(attr)
    # assert templ[attr] != list

    if val.lower() == "none":
        task[attr] = None
    else:
        val = convert_attr_val(attr, val)
        task[attr] = val

    lib.util._enforce_task_format(task)

def convert_attr_val(attr, val):
    templ = lib.util.task_template

    if attr in ["desc", "title"]:
        assert templ[attr] == str
        return val
    elif attr == "rank":
        try:
            return float(val)
        except ValueError:
            print(f"error: rank value {val} isn't convertable to float",
                  file=sys.stderr)
            sys.exit(-1)
    elif attr == "due":
        # Other date formats not yet supported... ez to add
        assert len(val) == 4
        date = datetime.now().date()
        year = int(date.strftime("%Y"))%100
        try:
            dt = datetime.strptime(f"18:00 {val}{year}", "%H:%M %d%m%y")
        except ValueError:
            print(f"error: unknown date format {val}")
            sys.exit(-1)
        due = lib.util.datetime_to_unix(dt)
        return due
    elif attr == "project":
        try:
            return [val]
        except ValueError:
            print(f"error: project value {val} isn't convertable to list",
                  file=sys.stderr)
            sys.exit(-1)
    else:
        print(f"error: unhandled attr '{attr}' = {val}")
        sys.exit(-1)

async def show_active_tasks():
    refids = await api.get_ref_ids()
    tasks = []
    for refid in refids:
        tasks.append(await api.fetch_task(refid))
    list_tasks(tasks, [])

async def show_deactive_tasks(month):
    tasks = await api.fetch_deactive_tasks(month)
    list_tasks(tasks, [])

def list_tasks(tasks, filters):
    headers = ["ID", "Title", "Status", "Project",
               "Tags", "assign", "Rank", "Due"]
    table_rows = []
    for id, task in enumerate(tasks, 1):
        if task is None:
            continue
        if is_filtered(task, filters):
            continue
        title = task["title"]
        status = task["state"]
        # project = task["project"] if task["project"] is not None else ""
        tags = " ".join(f"+{tag}" for tag in task["tags"])
        assign = " ".join(f"@{assign}" for assign in task["assign"])
        project = " ".join(f"{project}" for project in task["project"])
        if task["due"] is None:
            due = ""
        else:
            dt = lib.util.unix_to_datetime(task["due"])
            due = dt.strftime("%H:%M %d/%m/%y")

        rank = round(task["rank"], 4) if task["rank"] is not None else ""

        if status == "start":
            id =        Fore.GREEN + str(id)         + Style.RESET_ALL
            title =     Fore.GREEN + str(title)      + Style.RESET_ALL
            status =    Fore.GREEN + str(status)     + Style.RESET_ALL
            project =   Fore.GREEN + str(project)    + Style.RESET_ALL
            tags =      Fore.GREEN + str(tags)       + Style.RESET_ALL
            assign =    Fore.GREEN + str(assign)     + Style.RESET_ALL
            rank =      Fore.GREEN + str(rank)       + Style.RESET_ALL
            due =       Fore.GREEN + str(due)        + Style.RESET_ALL
        elif status == "pause":
            id =        Fore.YELLOW + str(id)        + Style.RESET_ALL
            title =     Fore.YELLOW + str(title)     + Style.RESET_ALL
            status =    Fore.YELLOW + str(status)    + Style.RESET_ALL
            project =   Fore.YELLOW + str(project)   + Style.RESET_ALL
            tags =      Fore.YELLOW + str(tags)      + Style.RESET_ALL
            assign =    Fore.YELLOW + str(assign)    + Style.RESET_ALL
            rank =      Fore.YELLOW + str(rank)      + Style.RESET_ALL
            due =       Fore.YELLOW + str(due)       + Style.RESET_ALL
        else:
            #id =        Style.DIM  + str(id)        + Style.RESET_ALL
            #title =     Style.DIM  + str(title)     + Style.RESET_ALL
            #status =    Style.DIM  + str(status)    + Style.RESET_ALL
            project =    Style.DIM  + str(project)   + Style.RESET_ALL
            tags =       Style.DIM  + str(tags)      + Style.RESET_ALL
            #assign =    Style.DIM  + str(assign)    + Style.RESET_ALL
            rank =       Style.DIM  + str(rank)      + Style.RESET_ALL
            due =        Style.DIM  + str(due)       + Style.RESET_ALL

        rank_value = task["rank"] if task["rank"] is not None else 0
        row = [
            id,
            title,
            status,
            project,
            tags,
            assign,
            rank,
            due,
        ]
        table_rows.append((rank_value, row))

    table = [row for (_, row) in
             sorted(table_rows, key=lambda item: item[0], reverse=True)]
    print(tabulate(table, headers=headers))

async def show_task(refid):
    task = await api.fetch_task(refid)
    task_table(task)
    return 0

async def show_archive_task(id, month):
    task = await api.fetch_archive_task(id, month)
    task_table(task)
    return 0

def tabulate_task(task):
    tags = " ".join(f"+{tag}" for tag in task["tags"])
    assign = " ".join(f"@{assign}" for assign in task["assign"])
    rank = round(task["rank"], 4) if task["rank"] is not None else ""
    if task["due"] is None:
        due = ""
    else:
        dt = lib.util.unix_to_datetime(task["due"])
        due = dt.strftime("%H:%M %d/%m/%y")

    assert task["created_at"] is not None
    dt = lib.util.unix_to_datetime(task["created_at"])
    created_at = dt.strftime("%H:%M %d/%m/%y")

    table = [
        ["Title:", task["title"]],
        ["Description:", task["desc"]],
        ["Status:", task["state"]],
        ["Project:", task["project"]],
        ["Tags:", tags],
        ["assign:", assign],
        ["Rank:", rank],
        ["Due:", due],
        ["Created:", created_at],
    ]
    return tabulate(table, headers=["Attribute", "Value"])

def task_table(task):
    print(tabulate_task(task))

    table = []
    for event in task["events"]:
        cmd, when, args = event[0], event[1], event[2:]
        when = lib.util.unix_to_datetime(when)
        when = when.strftime("%H:%M %d/%m/%y")
        if cmd == "set":
            who, attr, val = args
            if attr == "due" and val is not None:
                val = lib.util.unix_to_datetime(val)
                val = val.strftime("%H:%M %d/%m/%y")
            table.append([
                Style.DIM + f"{who} changed {attr} to {val}" + Style.RESET_ALL,
                "",
                Style.DIM + when + Style.RESET_ALL
            ])
        elif cmd == "append":
            who, attr, val = args
            if attr == "tags":
                val = f"+{val}"
            elif attr == "assign":
                val = f"@{val}"
            table.append([
                Style.DIM + f"{who} added {val} to {attr}" + Style.RESET_ALL,
                "",
                Style.DIM + when + Style.RESET_ALL
            ])
        elif cmd == "remove":
            who, attr, val = args
            if attr == "tags":
                val = f"+{val}"
            elif attr == "assign":
                val = f"@{val}"
            table.append([
                Style.DIM + f"{who} removed {val} from {attr}" + Style.RESET_ALL,
                "",
                Style.DIM + when + Style.RESET_ALL
            ])
        elif cmd == "state":
            who, status = args
            if status == "pause":
                status_verb = "paused"
            elif status in ["start", "cancel"]:
                status_verb = f"{status}ed"
            elif status == "stop":
                status_verb = f"stopped"
            else:
                print(f"internal error: unhandled task state {status}",
                      file=sys.stderr)
                sys.exit(-2)

            table.append([
                f"{who} {status_verb} task",
                "",
                Style.DIM + when + Style.RESET_ALL
            ])
    print(tabulate(table))

    table = []
    for event in task['events']:
        cmd, when, args = event[0], event[1], event[2:]
        when = lib.util.unix_to_datetime(when)
        when = when.strftime("%H:%M %d/%m/%y")
        if cmd == "comment":
            who, comment = args
            table.append([
                f"{who}>",
                wrap_comment(comment, 58),
                Style.DIM + when + Style.RESET_ALL
            ])
    if len(table) > 0:
        print("Comments:")
    print(tabulate(table))

def wrap_comment(comment, width):
    lines = []
    line_start = 0
    for i, char in enumerate(comment):
        if char == ' ' and (i - line_start >= width):
            lines.append(comment[line_start:i + 1])
            line_start = i + 1

    if line_start < len(comment):
        lines.append(comment[line_start:])
    return '\n'.join(lines)

async def modify_task(refid, args):
    changes = []
    for arg in args:
        if arg[0] == "+":
            tag = arg[1:]
            changes.append(("append", "tags", tag))
        # This must go before the next elif block
        elif arg.startswith("-@"):
            assign = arg[2:]
            changes.append(("remove", "assign", assign))
        elif arg[0] == "-":
            tag = arg[1:]
            changes.append(("remove", "tags", tag))
        elif arg[0] == "@":
            assign = arg[1:]
            changes.append(("append", "assign", assign))
        elif ":" in arg:
            attr, val = arg.split(":", 1)
            if val.lower() == "none":
                if attr not in ["project", "rank", "due"]:
                    print(f"error: invalid you cannot set {attr} to none",
                          file=sys.stderr)
                    return -1
                val = None
            else:
                val = convert_attr_val(attr, val)
            changes.append(("set", attr, val))
        else:
            print(f"warning: unknown arg '{arg}'. Skipping...", file=sys.stderr)
    await api.modify_task(refid, changes)
    return 0

async def change_task_status(refid, status):
    task = await api.fetch_task(refid)
    assert task is not None
    title = task["title"]

    if not await api.change_task_status(refid, status):
        return -1

    if status == "start":
        print(f"Started task '{title}'")
    elif status == "pause":
        print(f"Paused task '{title}'")
    elif status == "stop":
        print(f"Completed task '{title}'")
    elif status == "cancel":
        print(f"Cancelled task '{title}'")

    return 0

async def comment(refid, args):
    if not args:
        comment = prompt_comment_text()
    else:
        comment = " ".join(args)

    if not await api.add_task_comment(refid, comment):
        return -1

    task = await api.fetch_task(refid)
    assert task is not None
    title = task["title"]
    print(f"Commented on task'{title}'")
    return 0

def is_filtered(task, filters):
    for fltr in filters:
        if fltr.startswith("+"):
            tag = fltr[1:]
            if tag not in task["tags"]:
                return True
        elif fltr.startswith("@"):
            assign = fltr[1:]
            if assign not in task["assign"]:
                return True
        elif ":" in fltr:
            attr, val = fltr.split(":", 1)
            if val.lower() == "none":
                if attr not in ["project", "rank", "due"]:
                    print(f"error: invalid you cannot set {attr} to none",
                            file=sys.stderr)
                    sys.exit(-1)
                if task[attr] is not None:
                    return True
            elif attr == "state" :
                if val not in ["open", "start", "pause"]:
                    print(f"error: invalid, filter by {attr} can only be [\"open\", \"start\", \"pause\"]",
                            file=sys.stderr)
                    sys.exit(-1)
                if task["state"] != val:
                    return True
            elif attr == "project":
                if task["project"] is None:
                    return True
                if not task["project"].startswith(val):
                    return True
            else:
                val = convert_attr_val(attr, val)
                if task[attr] != val:
                    return True
        else:
            print(f"error: unknown arg '{fltr}'", file=sys.stderr)
            sys.exit(-1)

    return False

def find_free_id(task_ids):
    for i in range(1, 1000):
        if i not in task_ids:
            return i
    1

def map_ids(task_ids, ref_ids):
    return dict(zip(task_ids, ref_ids))

async def main():
    refids = await api.get_ref_ids()
    free_ids = []
    tasks = []
    for refid in refids:
        tasks.append(await api.fetch_task(refid))
        free_ids.append(find_free_id(free_ids))

    data = map_ids(free_ids, refids)    

    if len(sys.argv) == 1:
        await show_active_tasks()
        return 0

    if sys.argv[1] in ["-h", "--help", "help"]:
        print('''USAGE:
    tau [OPTIONS] [SUBCOMMAND]

OPTIONS:
    -h, --help                   Print help information

SUBCOMMANDS:
    add        Add a new task.
    archive    Show completed tasks.
    comment    Write comment for task by id.
    modify     Modify an existing task by id.
    pause      Pause task(s).
    start      Start task(s).
    stop       Stop task(s).
    help       Show this help text.

Example:
    tau add task one due:0312 rank:1.022 project:zk +lol @sk desc:desc +abc +def
    tau add task two  rank:1.044 project:cr +mol @up desc:desc2
    tau add task three due:0512 project:zy +trol @kk desc:desc3 +who
    tau 1 modify @upgr due:1112 rank:none
    tau 1 modify -mol -xx
    tau 2 start
    tau 1 comment "this is an awesome comment"
    tau 2 pause
    tau archive         # current month's completed tasks
    tau archive 1122    # completed tasks in Nov. 2022
    tau 0 archive 1122  # show info of task completed in Nov. 2022
''')
        return 0
    elif sys.argv[1] == "add":
        task_args = sys.argv[2:]
        await add_task(task_args)
        return 0
    elif sys.argv[1] == "archive":
        if len(sys.argv) > 2:
            if len(sys.argv[2]) == 4:
                month = sys.argv[2]
            else:
                print("error: month must be of format MMYY")
                return -1
        else:
            month = lib.util.current_month()

        await show_deactive_tasks(month)
        return 0
    elif sys.argv[1] == "show":
        if len(sys.argv) > 2:
            filters = sys.argv[2:]
            list_tasks(tasks, filters)
        else:
            await show_active_tasks()
        return 0

    try:
        id = int(sys.argv[1])
        refid = data[id]
    except ValueError:
        print("error: invalid ID", file=sys.stderr)
        return -1

    args = sys.argv[2:]

    if not args:
        return await show_task(refid)

    subcmd, args = args[0], args[1:]

    if subcmd == "modify":
        if (errc := await modify_task(refid, args)) < 0:
            return errc
        return await show_task(refid)
    elif subcmd in ["start", "pause", "stop", "cancel"]:
        status = subcmd
        if (errc := await change_task_status(refid, status)) < 0:
            return errc
    elif subcmd == "comment":
        if (errc := await comment(refid, args)) < 0:
            return errc
    elif subcmd == "archive":
        if len(args) == 1:
            if len(args[0]) == 4:
                month = args[0]
            else:
                print("Error: month must be of format MMYY")
                return -1
        else:
            month = lib.util.current_month()

        if (errc := await show_archive_task(refid, month)) < 0:
            return errc
    else:
        print(f"error: unknown subcommand '{subcmd}'")
        return -1

    return 0

asyncio.run(main())

