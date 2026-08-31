# Contributing With Tor

... or how to setup Tor git access with darkfi repo.

We assume you have tor installed locally. You can check your tor daemon
is running by running this command:

```shell
$ curl --socks5-hostname 127.0.0.1:9050 https://myip.wtf/text
```

<!--
```py
# install pysocks
import socks
import socket
socks.set_default_proxy(socks.SOCKS5, "127.0.0.1", 9050)
socket.socket = socks.socksocket
import requests
response = requests.get("https://myip.wtf/text")
print(response.content)
```
-->

## Setting Up Git Repo

Generate a brand new SSH key using the command:

```shell
$ ssh-keygen -o -a 100 -t ed25519 -f ~/.ssh/anon_tor -C x@x
```

Configure a new SSH host by adding a section in `~/.ssh/config` like
this:

```text
Host darkfi-git-tor
    # Use this for debugging errors
    #LogLevel VERBOSE
    User git
    HostName moxcgg4oghre3tphoxzkywoutcyqogzntb7webitaxzqtd6i43gkjcqd.onion
    IdentitiesOnly yes
    IdentityFile ~/.ssh/anon_tor
    ProxyCommand nc -x 127.0.0.1:9050 %h %p
```

> Note:
>
> You will need BSD netcat installed. Optionally you could use GNU
> netcat, but the flags are different; replace `-x` with
> `--proxy ... --proxy-type=socks5`.

Clone a fresh repo:

```shell
$ git clone git@darkfi-git-tor:darkrenaissance/darkfi
$ cd darkfi
```

It is highly recommended to remove older repo folders. You don't want to
accidentally `git push` to a clearnet remote and dox yourself.

## Git Config

You can still be identified by your machine's Git config, if pushing to
external repos on clearnet. However, we can set per project settings,
so inside the darkfi repo, run these commands:

```shell
$ git config --local user.name x
$ git config --local user.email x@x
$ git config --local commit.gpgsign false
```

Verify it has been set with:

```shell
$ cat .git/config
```

### Commit Timestamps

`git` commits contain two timestamps, the `AuthodDate` and the
`CommitDate`. These timestamps are retrieved from the system and
contain the configured timezone. If you want to exclude the timezone
information from your commits, you may create a `git` alias to use:

```shell
$ git config --global alias.utc-commit \
'!GIT_COMMITTER_DATE="$(date --utc +%Y-%m-%dT%H:%M:%S%z)" git commit --date="$(date --utc +%Y-%m-%dT%H:%M:%S%z)"'
```

This allows to explictly use `UTC` date on commits by executing:

```shell
$ git utc-commit -m "{Commit message}"
```

Alternative, you can use the same alias in your shell configuration for
`git commit` to always use `UTC` date on all your repos commits.

In order to ensure that all `git` operations are using a `UTC`
timestamp you can set your terminal to `UTC` date by executing:

```shell
$ export TZ=UTC0
```

Alternative, add the command in your shell configuration so all your
terminal instances always use `UTC` date by default.

## Cargo config

When building `rust` projects `cargo` is used to fetch external crates.
To configure it to use tor add a section in `~/.cargo/config` like
this:

```toml
[http]
proxy = "socks5h://localhost:9050"
```

Now even building goes through tor and a developer never touches
clearnet at any point.

> Note:
>
> If you are using VMs/containers for building, ensure that they also
> go through tor, as above configuration is only relevant for native
> system building.
