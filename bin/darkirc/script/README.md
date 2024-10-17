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

## `taubot`
sends notifications about some `tau` commands (namely: adding or 
stopping a task, changing state, reassigning and new comments) to 
desired channels in `darkirc`. 

## `mirror-bot`
sends a copy of the message sent in an IRC server to another.

## `commitbot`
sends notifications about pushed code to `github` to desired channels 
in `darkirc`. 

## `test-bot`
sends a response to user sent 'test' or 'echo' `PrivMsg`s.

## Setup

* Create a new darkirc config for the bot.
* Edit the config for your needs.
* Run a `darkirc` instance, make sure to point it to the new config.
* Run the bot script.


## Run

### Using a venv

All these bots are working on and require Python 3.12.7. Make sure 
Python is installed and on the latest version.

Depending on your setup you may need to install a virtual environment
for Python. Do so as follows:

```shell
% python -m venv venv_{bot_name}
% source venv_{bot_name}/bin/activate
```

Then install the requirements:

```shell
% pip install -r requirements_{bot_name}.txt
```

Run bot:

```shell
% python {bot_script}.py
```

You will need to reactivate the venv in your current terminal session
each time you use the bot as follows:

```shell
% source venv_{bot_name}/bin/activate
```

### Without a venv

If you don't require a venv, install the requirements and run needed bot as follows:

```shell
% pip install -r requirements.txt
% python {bot_script}.py
```

**Notes:**
* Make sure the `irc_listen` is the same in config and in bot script.
