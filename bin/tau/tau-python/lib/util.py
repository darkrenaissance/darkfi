import random, time
from datetime import UTC, datetime

def random_blob_idx():
    return "%030x" % random.randrange(16**30)

def datetime_to_unix(dt):
    return int(time.mktime(dt.timetuple()))
def now():
    return datetime_to_unix(datetime.now(tz=UTC))

def month_to_unix(month=None):
    month_year = month if month is not None else datetime.utcnow().strftime("%m%y")
    return datetime.strptime(month_year,"%m%y").timestamp()

def unix_to_datetime(timestamp):
    return datetime.utcfromtimestamp(int(timestamp))

task_template = {
    "workspace": str,
    "title": str,
    "tags": list,
    "desc": str,
    "owner": str,
    "assign": list,
    "project": list,
    "due": int,
    "rank": float,
    "created_at": int,
    "state": str,
    "events": list,
    "comments": list,
}

def _enforce_task_format(task):
    for attr, val in task.items():
        val_type = task_template[attr]
        if val is None:
            assert val_type == list or attr not in ["created"]
            continue
        assert isinstance(val, val_type)

