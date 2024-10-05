# Drop this in .local/share/weechat/python/autoload/
import weechat

weechat.register("buffclone", "narodnik", "1.0", "GPL3",
                 "Clones messages from channel list into a single buffer",
                 "", "")
weechat.prnt("", "Hello, from python script!")

buff = weechat.buffer_new("darkfi-all", "", "", "", "")
weechat.buffer_set(buff, "title", "darkfi-all")
weechat.buffer_set(buff, "localvar_set_no_log", "1")

weechat.hook_print("", "", "", 0, "on_print", "")

def on_print(data, buffer, date, tags, displayed, highlight, prefix, message):
    # Get channel
    channel = weechat.buffer_get_string(buffer, "name")
    if channel.startswith("darkfi."):
        channel = channel.removeprefix("darkfi.")
    else:
        # Ignore other channels. Too much noise.
        return weechat.WEECHAT_RC_OK

    if channel.startswith("#lunardao"):
        return weechat.WEECHAT_RC_OK

    # Get nick
    tags = tags.split(",")
    nicks = [tag for tag in tags if tag.startswith("nick_")]
    # Ignore non-messages
    if len(nicks) != 1:
        return weechat.WEECHAT_RC_OK
    nick = nicks[0].removeprefix("nick_")

    buffstr = f"{channel}" + (20 - len(channel))*" "
    buffstr += f"<{nick}>" + (18 - len(nick))*" "
    buffstr += f"{message}"

    weechat.prnt(buff, buffstr)
    return weechat.WEECHAT_RC_OK

