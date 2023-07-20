import matplotlib.pyplot as plt
import numpy as np
import os
import glob

darkies = []
idx = 0
for darkie in glob.glob('log/darkie[0-9]*.log'):
    with open(darkie) as f:
        buf = f.read()
        lines = buf.split('\n')
        apr = float(lines[2].split(':')[1].strip())
        initial_stake = [float(item) for item in lines[0].split(':')[1].split(',')]
        idx +=1
        if sum(initial_stake)!=0 :
            darkies += [(initial_stake, apr, idx)]
# plot initial stake

for darkie in darkies:
    plt.plot(darkie[0])
    plt.title('initial stake')


legends = []
for darkie in darkies:
    legend = ["darkie{}".format(darkie[2])]
    legends +=[legend]
plt.legend(legends)
plt.savefig("log/plot_darkies_is.png")

# plot apr

for darkie in darkies:
    plt.plot(darkie[1])
    plt.title('apr')


legends = []
for darkie in darkies:
    legend = ["darkie{}".format(darkie[2])]
    legends +=[legend]
plt.legend(legends)
plt.savefig("log/plot_darkies_apr.png")
