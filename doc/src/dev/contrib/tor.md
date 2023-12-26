# Contributing With Tor

... or how to setup Tor git access with darkfi repo.

We assume you have tor installed locally and access to Tor browser.
You can check your tor daemon is running by installing `python-pysocks`
and then run this command:

```
curl --socks5-hostname 127.0.0.1:9050 https://myip.wtf/text
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

## Setting Up Codeberg

Follow these steps:

1. Generate a new SSH key using the command:
   `ssh-keygen -o -a 100 -t ed25519 -f ~/.ssh/id_tor -C foo@foo`
2. Next use Tor Browser to make a codeberg account, and get this added to the
   darkfi repo.
3. Add your key `.ssh/id_tor.pub` to your account on codeberg.
4. Verify your key by signing the message:
   `echo -n 'XXX' | ssh-keygen -Y sign -n gitea -f ~/.ssh/id_tor`
   where XXX is the string given on codeberg.

## SSH Config

You will need BSD netcat installed. Optionally you could use GNU netcat, but
the flags are different; replace `-x` with `--proxy ... --proxy-type=socks5`.

Add a section in `~/.ssh/config` like this:

```
Host codeberg-tor
    # Use this for debugging errors
    #LogLevel VERBOSE
    User git
    HostName codeberg.org
    IdentitiesOnly yes
    IdentityFile ~/.ssh/id_tor
    ProxyCommand nc -x 127.0.0.1:9050 %h %p
```

Then test it is working with `ssh -T git@codeberg-tor -vvv`.
Be sure to verify the signatures match those on the codeberg website.
To see them, copy this link into Tor Browser:
https://docs.codeberg.org/security/ssh-fingerprint/

## Adding the Git Remote

The last step is routine:

```
git remote add codeberg git@codeberg-tor:darkrenaissance/darkfi.git
```

And then finally it should work.
