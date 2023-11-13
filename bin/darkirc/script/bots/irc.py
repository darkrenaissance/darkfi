import socket

class IRC:
    irc = socket.socket()
  
    def __init__(self):
        # Define the socket
        self.irc = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
 
    def send(self, channel, msg):
        # Transfer data
        self.irc.send(bytes("PRIVMSG " + channel + " :" + msg + "\n", "UTF-8"))
 
    def connect(self, server, port, channels, botnick):
        # Connect to the server
        print("Connecting to: " + server)
        self.irc.connect((server, port))

        # Perform user authentication
        self.irc.send(bytes("CAP LS 302\n", "UTF-8"))
        self.irc.send(bytes("CAP REQ :no-history\n", "UTF-8"))
        self.irc.send(bytes("NICK " + botnick + "\n", "UTF-8"))
        self.irc.send(bytes("USER " + botnick + " 0 * :" + botnick + "\n", "UTF-8"))
        

        # join the channel
        for chan in channels:
            self.irc.send(bytes("JOIN " + chan + "\n", "UTF-8"))
 
    def get_response(self):
        # Get the response
        resp = self.irc.recv(2040).decode("UTF-8")
        msg = resp.split(':')[-1]
 
        if resp.find('PING') != -1:                      
            self.irc.send(bytes('PONG ' + msg + '\r\n', "UTF-8")) 
 
        return resp.strip()
