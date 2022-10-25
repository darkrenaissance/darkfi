import json
import collections

import matplotlib.pyplot as plt


f = open("./serial_merkle_time.json")

def load_data(path):
    f = open(path)
    
    data = json.load(f)
    data = {int(k):v for k,v in data.items()}
    od = collections.OrderedDict(sorted(data.items()))
    
    
    coins = list(od.keys()) 
    seconds =  [ v.get("secs") + (v.get("nanos") / 1e9) for v in od.values()]

    return coins, seconds

plt.title(f"Serialize/Deserialize merkle tree benchmark")
coins, seconds = load_data("./serial_merkle_time.json")
plt.plot(coins, seconds, label='serializing')
coins, seconds = load_data("./deserial_merkle_time.json")
plt.plot(coins, seconds, label='deserializing')

plt.legend()
plt.ylabel(f"time in seconds")
plt.xlabel("number of coins")
plt.savefig("serial_desrial_merkle_time.png")

#plt.title(f"Serialize/Deserialize nullifiers vector benchmark")
#coins, seconds = load_data("./serial_null_time.json")
#plt.plot(coins, seconds, label='serializing')
#coins, seconds = load_data("./deserial_null_time.json")
#plt.plot(coins, seconds, label='deserializing')
#
#plt.legend()
#plt.ylabel(f"time in seconds")
#plt.xlabel("number of coins")
#plt.savefig("serial_desrial_null_time.png")
