
import argparse

async def arg_parser(client):
    parser = argparse.ArgumentParser(
            prog='drk',
            usage='%(prog)s [commands]',
            description="""DarkFi wallet command-line tool"""
            )

    parser.add_argument('-c', '--cashier', action='store_true', help='Create a cashier wallet')
    parser.add_argument('-w', '--wallet', action='store_true', help='Create a new wallet')
    parser.add_argument('-k', '--key', action='store_true', help='Test key')
    parser.add_argument('-i', '--info', action='store_true', help='Request info from daemon')
    parser.add_argument('-hi', '--hello', action='store_true', help='Test hello')
    parser.add_argument("-s", "--stop", action='store_true', help="Send a stop signal to the daemon")

    try:
        args = parser.parse_args()

        if args.key:
            print("Attemping to generate a create key pair...")
            await client.key_gen(client.payload)

        if args.wallet:
            print("Attemping to generate a create wallet...")
            await client.create_wallet(client.payload)

        if args.info:
            print("Info was entered")
            await client.get_info(client.payload)
            print("Requesting daemon info...")

        if args.stop:
            print("Stop was entered")
            await client.stop(client.payload)
            print("Sending a stop signal...")

        if args.hello:
            print("Hello was entered")
            await client.say_hello(client.payload)

        if args.cashier:
            print("Cash was entered")
            await client.create_cashier_wallet(client.payload)

    except Exception:
        raise

    #subparser = parser.add_subparsers(help='All available commands', title="Commands", dest='cmd')
    #subparser.metavar = 'subcommands';
    #login = subparser.add_parser('login', help='wallet login')
    ##test = subparser.add_parser('test', help='test wallet functions')
    #new = subparser.add_parser('new', help='create something new')

    #new.add_argument('-w', '--wallet', action='store_true', help='Create a new wallet')
    #new.add_argument('-k', '--key', action='store_true', help='Create a new key')
    #new.add_argument('-c', '--cashier', action='store_true', help='Create a cashier wallet')

    #login.add_argument('-u', '--username', type=str, required=True)
    #login.add_argument('-p', '--password', type=str, required=True)

    ##test.add_argument('-k', '--key', dest='key', action='store_true', help='Test key')
    ##test.add_argument('-p', '--path', dest='path', action='store_true', help='Test path')
    ##test.add_argument('-pk', '--pkey', dest='pkey', action='store_true', help='Print test key')
    ##test.add_argument('-ck', '--ckey', dest='ckey', action='store_true', help='Cashier test key')
    ##test.add_argument('-w', '--wallet', dest='wallet', action='store_true', help='Create a new wallet')
    ##test.add_argument('-c', '--cashier', dest='cashier',action='store_true', help='Create a cashier wallet')

    #if args.path:
    #    try:
    #        print("Testing path...")
    #        client.test_path(client.payload)
    #    except Exception:
    #        raise

    #if args.pkey:
    #    try:
    #        print("Attempting to print cashier key...")
    #        client.cashkey(client.payload)
    #    except Exception:
    #        raise



