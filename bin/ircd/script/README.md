IRC Bots
========

## `meetbot`

`meetbot.py` is a bot used to keep topics for IRC channels, that can
be discussed on meetings. Multiple channels can be configured and this
is done through `meetbot_cfg.py`.

**Notes:**
* Never add secrets to the public repo config!

### Setup

* Download `meetbot.py` and `meetbot_cfg.py` 
* Edit `meetbot_cfg.py` for your needs.
* Navigate terminal to the folder where `meetbot.py` is.
* Run the bot: `$ python meetbot.py`

## `titlebot`
`titlebot.py` is a bot used to print the title of a website provided by
a link in a `PRIVMSG`.

## `tweetifier`
`tweetifier` is yet another bot that recognizes Twitter links, fetch
the tweet text and print it out in irc.

## Requirment
 `git clone https://github.com/Dastan-glitch/tweety.git`\
 `cd tweety && pip install .`