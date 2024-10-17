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

### Requirements
 `pip install beautifulsoup4 requests`

## `tweetifier`
`tweetifier` is yet another bot that recognizes Twitter links, fetch
the tweet text and print it out in irc.

### Requirments
 `pip install tweety-ns`

## `taubot`
sends notifications about some `tau` commands (namely: adding or 
stopping a task, changing state, reassigning and new comments) to 
desired channels in `darkirc`. 

### Requirements
 `pip install argparse`

## `mirror-bot`
sends a copy of the message sent in an IRC server to another.

### Requirements
 `pip install argparse`

## `commitbot`
sends notifications about pushed code to `github` to desired channels 
in `darkirc`. 

### Requirements
 - `pip install https.server`
 - github webhook

 ## `test-bot`
sends a response to user sent 'test' or 'echo' `PrivMsg`s.

## Setup

* Create a new darkirc config for the bot.
* Edit the config for your needs.
* Run a `darkirc` instance, make sure to point it to the new config.
* Run the bot script.

**Notes:**
* Make sure the `irc_listen` is the same in config and in bot script.
