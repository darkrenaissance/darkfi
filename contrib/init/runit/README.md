Startup services for runit.

# Make DarkFi Binaries Available

Compile `taud` and `darkirc`. Make sure they are findable in your path.
You can copy the binaries to `~/.local/bin/`.

# Configure Per-User Services and Logging

Create the service dirs:
```bash
    $ mkdir ~/.local/service/
    $ mkdir ~/.local/sv/
```
`~/.local/sv/` will contain our services which we symlink into `~/.local/service/` to activate.

Now follow these guides:

* [Per-User Services](https://docs.voidlinux.org/config/services/user-services.html)
* [Logging](https://docs.voidlinux.org/config/services/logging.html)

To view the logs, open another terminal, switch to root and run the command below.
Leave this window open so we can view the daemon logs.
```bash
    # svlogtail daemon
```

Finally make sure your user level service is running:
```bash
    # sv status runsvdir-USER
    run: runsvdir-USER: (pid 25140) 483s
```

# Copy Services

Copy the directories here over to `~/.local/sv/`:
```bash
    $ cp -r darkirc/ taud/ ~/.local/sv/
```

Activate them by symlinking them into `~/.local/service/`. We need the full path.
```bash
    $ cd ~/.local/service/
    $ ln -s /home/USER/.local/sv/darkirc/ .
    $ ln -s /home/USER/.local/sv/taud/ .
```

Now check they are working fine:
```bash
    $ SVDIR=~/.local/sv/ sv status darkirc
```

You should also view the log output in the window we opened earlier.

# Short Explanation

Each directory contains an executable `run` script which launches the daemon.
We must redirect STDERR to STDOUT to get error messages in the log output.

To enable the log output, we must provide the `log/run` script which contains
the logger command. You can test logger output like this:
```bash
    $ vlogger -t darky -p daemon hello123
```
