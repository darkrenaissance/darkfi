import matplotlib.pyplot as plt

a = []
for i in range(-500, 500, 1):
    row = []
    for j in range(-500, 500, 1):
        x, y = float(j), float(i)
        x /= 2000
        y /= 2000
        v = y**2 - x**3 - 5
        #if 0.98 < x < 1.02 and 2.4 < y < 2.6:
        #    print(v)
        v = int(v * 1000)
        row.append(v)
    a.append(row)
plt.imshow(a, cmap='hot', interpolation='nearest')
plt.show()
