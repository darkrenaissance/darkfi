config = {
    # IRC server host
    "host": "127.0.0.1",

    # IRC server port
    "port": 6667,

    # IRC nickname
    "nick": "meetbot",
    "channels": [
        {
            "name": "#foo",
            "secret": None,
        },
        {
            "name": "#secret_channel",
            # TODO: This is useless right now, but it would be nice
            # to add a CAP in ircd to give all incoming PRIVMSG to be
            # able to check them.
            "secret": "HNEKcUmwsspdaL9b8sFn45b8Rf3bzv1LdYS1JVNvkPGL",
        },
    ],
}
