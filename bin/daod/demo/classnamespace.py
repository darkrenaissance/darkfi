from types import SimpleNamespace

class ClassNamespace(SimpleNamespace):
    def __init__(self, dic=None):
        if dic is None:
            return
        # if type(dic) is dict:
        for key in dic:
            setattr(self, key, self.envelop(dic[key]))
        # else:
        #     raise CatalogError("ClassNamespace AIUTO!")

    def envelop(self, elem):
        if type(elem) is dict:
            return ClassNamespace(elem)
        elif type(elem) is list:
            return [self.envelop(x) for x in elem]
        else:
            return elem

        # if d is not None:
        #     for key in d:
        #         if type(d[key]) is dict:
        #             setattr(self, key, ClassNamespace(d[key]))
        #         else:
        #             setattr(self, key, d[key])

    def __contains__(self, x):
        return x in self.__dict__

    def __json__(self, x):
        return self.__dict__

    def copy(self):
        return self.__dict__.copy()

    def classcopy(self):
        dummy = ClassNamespace()
        dummy.__dict__.update(self.__dict__)
        return dummy

    def dictcopy(self):
        return self.__dict__.copy()

    def update(self, oth):
        self.__dict__.update(oth.__dict__)
