import socket
import time

def main():
    stream = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    host = "127.0.0.1"  
    port = 11066 
    stream.connect((host, port))

    nick_msg = b"NICK MEETING \r\n"
    stream.send(nick_msg)

    join_msg = b"JOIN #dev \r\n"
    stream.send(join_msg)

    while True:
        time.sleep(6)    
        msg = b"PRIVMSG #dev :hello\r\n"
        stream.send(msg)



if __name__ == "__main__":
    main()
