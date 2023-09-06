/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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
