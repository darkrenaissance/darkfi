```python
sage: x
x
sage: l_2 = (x - 1)*(x - 3)*(x - 4)
sage: l_2
(x - 1)*(x - 3)*(x - 4)
sage: l_2(1)
0
sage: l_2(3)
0
sage: l_2(4)
0
sage: l_2(2)
2
sage: l_2
(x - 1)*(x - 3)*(x - 4)
sage: l_2 /= (2-1)*(2-3)*(2-4)
sage: l_2(2)
1
sage: l_3 = (x - 1)*(x - 2)*(x-4) / ((3-1)*(3-2)*(3-4))
sage: l_3(1)
0
sage: l_3(2)
0
sage: l_3(3)
1
sage: l_3(4)
0
sage: l_1 = (x - 2)*(x - 3)*(x - 4) / ((1 - 2)*(1 - 3)*(1
....: - 4))
sage: l_1(1)
1
sage: l_1(2)
0
sage: l_1(3)
0
sage: l_1(4)
0
sage: l_1
-1/6*(x - 2)*(x - 3)*(x - 4)
sage: l_2
1/2*(x - 1)*(x - 3)*(x - 4)
sage: l_3
-1/2*(x - 1)*(x - 2)*(x - 4)
sage: l_4
----------------------------------------------------------
NameError                Traceback (most recent call last)
<ipython-input-24-cbdd82c87bb0> in <module>
----> 1 l_4

NameError: name 'l_4' is not defined
sage: l_4 = (x - 1)*(x - 2)*(x - 3) / ((4 - 1)*(4 - 2)*(4
....: - 3))
sage: l_4(1)
0
sage: l_4(2)
0
sage: l_4(3)
0
sage: l_4(4)
1
sage: for i in range(1, 5):
....:     print(i, l_1(i), l_2(i), l_3(i), l_4(i))
....: 
1 1 0 0 0
2 0 1 0 0
3 0 0 1 0
4 0 0 0 1
sage: f = 6 * l_1 + 4 * l_2 + 3 * l_3 + 0 * l_4
sage: l_2(2)
1
sage: for i in range(1, 5):
....:     print(i, f(i))
....: 
1 6
2 4
3 3
4 0
```

