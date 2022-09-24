# IRC Private Channel Bot

*meeting_bot_secret.py* is an upgraded version of the *meeting_bot.py* for IRC. This version allows for multiple channels, including private ones.

**Note:**

* `{user}` needs to be exchanged with your *username*.
* Every channel runs a bot on a different thread - be careful how many bots you deploy!


**Setup**

* Donwload  *meeting_bot_secret.py* and *meeting_bot_secret_config.py* 
* Copy meeting_bot_secret_config.py to `/home/{user}/.config/darkfi`
* Open the config and set up all the channels and the values of name and secret (if not private, secret must be {null})
* Change `{user}` to your *username* in *meeting_bot_secret.py* in the path of `load_config()` function
* Navigate terminal to the folder where is *meeting_bot_secret.py*
* Run the bot: `$ python meeting_bot_secret.py`
