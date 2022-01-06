
class Method:
    def __init__(self, name, params):
        self.name = name
        self.params = params.replace(',', ', ');
        self.result = ""
        self.note = ""


    def set_result(self, result):
        self.result = result.replace(':', ': ');
        self.result = result.replace(',', ', ');

    def __str__(self):
        method_str = '### ' + self.name + ': \n' \
                + '`params`: ' +  self.params + '\n' \
                + '\n' \
                + '`result`: ' +  self.result + '\n' \
                + '\n' \

        if not self.note == "":
            method_str += '> `note`: ' +  self.note + '\n' 

        return method_str


def main():
    methods =  []
    with open('../src/bin/darkfid.rs') as f:
        lines = f.readlines()
        for i in range(0, len(lines)):
            line = lines[i]

            if line.__contains__("RPCAPI"):

                line = lines[i + 1]
                if line.__contains__(' --> '):
                    line = line.strip()
                    words = line.split(' ')
                    method = words[3][1::][:-2:]
                    params = words[5][:-1:]
                    methods.append(Method(method, params))

                line = lines[i + 2]
                if line.__contains__(' <-- '):
                    line = line.strip()
                    words = line.split(' ')
                    methods[-1].set_result(words[3][:-1:])

                line = lines[i + 3]
                if line.__contains__("APINOTE"):
                    methods[-1].note = line.strip().replace('// APINOTE:','')
                    count = i + 4
                    line = lines[count]
                    while line.strip().startswith('//'):
                        count += 1
                        methods[-1].note += line[6::]
                        line = lines[count]



    with open('src/clients/jsonrpc.md', 'w') as f:
        f.write('# JSONRPC API \n')
        f.write('## Methods \n')
        for m in methods:
            f.write('- [' + m.name + '](jsonrpc.md#' + m.name + ')\n')
        for m in methods:
            f.write(m.__str__())


if __name__ == '__main__':
    main()
