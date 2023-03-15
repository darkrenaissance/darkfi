import numpy as np
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d import Axes3D

fig = plt.figure()
ax = fig.add_subplot(projection='3d', azim=30)
with open("gains.txt", "r") as f:
    buf = f.read().split('\n')
    KI = []
    KP = []
    KD = []
    ACC = []
    for line in buf:
        if len(line)<4:
            continue
        cord  = [round(float(i),2) for i in line.split(',')]
        acc = cord[0]
        kp = cord[1]
        ki = cord[2]
        kd = cord[3]
        KI+=[ki]
        KP+=[kp]
        KD+=[kd]
        ACC+=[acc]
img = ax.scatter(KP, KI, KD, c=ACC, cmap=plt.hot())
ax.set_xlabel("KP")
ax.set_ylabel("KI")
ax.set_zlabel("KD")
fig.colorbar(img)
plt.savefig("heuristics.png")
#plt.show()
