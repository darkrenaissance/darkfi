import datetime as dt

def isotime_to_ms(isotime):
    ms = dt.timedelta(microseconds=1)
    time = dt.time.fromisoformat(isotime)
    ms_time = (dt.datetime.combine(dt.date.min, time) - dt.datetime.min) / ms
    return ms_time

def line_time(line):
    return isotime_to_ms(line.split()[1])

lines = []
filenames = {"Client": "/tmp/a.txt", "Server": "/tmp/b.txt"}

for label, filename in filenames.items():
    with open(filename) as file:
        file_lines = file.read().split("\n")
        # Cleanup a bit
        file_lines = [line for line in file_lines if line and line[0].isdigit()]
        # Attach the label to each line
        file_lines = ["%s: %s" % (label, line) for line in file_lines]
        lines.extend(file_lines)

lines.sort(key=line_time)
for line in lines:
    # Now remove timestamps and other info we don't need
    line = line.split()
    line = line[0] + " " + " ".join(line[4:])
    print(line)
