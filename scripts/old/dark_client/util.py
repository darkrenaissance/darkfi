
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
    parser.add_argument("-t", "--test", action='store_true', help="Test writing to the wallet")

    try:
        args = parser.parse_args()

        if args.key:
            print("Attemping to generate a create key pair...")
            await client.key_gen(client.payload)

        if args.wallet:
            print("Attemping to create a wallet...")
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
            print("Attempting to generate a cashier wallet...")
            await client.create_cashier_wallet(client.payload)

        if args.test:
            print("Testing wallet write")
            await client.test_wallet(client.payload)

    except Exception:
        raise
