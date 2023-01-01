import glob, re, os.path, textwrap
from colorama import Fore, Back, Style

def mod_prefix(dir):
    if dir.startswith("net"):
        return "net"
    elif dir.startswith("serial"):
        return "serial"
    elif dir.startswith("util"):
        return "util"
    elif dir.startswith("runtime"):
        return "runtime"
    elif dir.startswith("zk"):
        return "zk"
    elif dir.startswith("raft"):
        return "raft"
    elif dir.startswith("sdk/src/crypto"):
        return "sdk::crypto"
    elif dir.startswith("sdk"):
        return "sdk"
    elif dir.startswith("contract/dao"):
        return "dao"
    elif dir.startswith("contract/money"):
        return "money"
    elif dir.startswith("rpc"):
        return "rpc"
    elif dir.startswith("system"):
        return "system"
    elif dir.startswith("dht"):
        return "dht"
    elif dir.startswith("consensus"):
        return "consensus"
    elif dir.startswith("zkas"):
        return "zkas"
    elif dir.startswith("blockchain"):
        return "blockchain"
    elif dir.startswith("wallet"):
        return "wallet"
    else:
        assert not dir or dir == "tx"
        return ""

def mod_suffix(prefix, base):
    if prefix in ("dao", "money"):
        return ""
    elif base in ("mod.rs", "lib.rs"):
        return ""
    return base.removesuffix(".rs")

def log_target(fname):
    dir, base = os.path.dirname(fname), os.path.basename(fname)
    prefix = mod_prefix(dir)
    suffix = mod_suffix(prefix, base)
    # you don't need :: when the suffix is empty
    if not suffix and not prefix:
        return ""
    if not suffix:
        return prefix
    if not prefix:
        return suffix
    return f"{prefix}::{suffix}"

def replace(fname, contents):
    target = log_target(fname)
    # You can debug like this:
    #if target != "consensus::proposal":
    #    return ""
    print(f"Replacing {target}" + " "*(40 - len(target)) + f"[{fname}]")

    result = ""
    lines = contents.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]

        # only used for debug output
        old_line = None
        new_line = None
        # This is used as a debug goto
        line_modified = False

        log_level = None
        if "trace!(" in line:
            log_level = "trace"
        elif "debug!(" in line:
            log_level = "debug"
        elif "info!(" in line:
            log_level = "info"
        elif "warn!(" in line:
            log_level = "warn"
        elif "error!(" in line:
            log_level = "error"

        if log_level is not None:
            if not target:
                # No target exists for this file at all. Just ignore these
                # We would normally delete any target set for these files
                # but so far we have none of them, so just ignore them.
                print(
                    "    "
                    + Back.RED + "Skip [no target]:" + Style.RESET_ALL
                    + f" {line}"
                )
            elif re.search(f'{log_level}!\\(target: ([A-Z_]+|"[a-zA-Z:_-]+"),', line):
                # Single lines with a target that's a constant or string
                old_line = f"{i}: {line}"

                line = re.sub(
                     'target: ([A-Z_]+|"[a-zA-Z:_-]+"),',
                    f'target: "{target}",',
                    line
                )

                line_modified = True
                new_line = f"{i}: {line}"
            elif f'{log_level}!("' in line:
                # Normal single lines with no target set
                old_line = f"{i}: {line}"

                #print(f"    No target: {line}")
                line = line.replace(f'{log_level}!(',
                                    f'{log_level}!(target: "{target}", ')

                line_modified = True
                new_line = f"{i}: {line}"
            else:
                # Multiline logs
                # We read the next line and check if there's a target set or not
                old_line = f"{i}: {line}"
                new_line = f"{i}: {line}"

                result += line + "\n"
                i += 1
                assert i < len(lines)
                line = lines[i]

                old_line += f"\n{i}: {line}"

                if re.search('target: ([A-Z_]+|"[a-zA-Z:_-]+"),', line):
                    # Constants or target strings set
                    line = re.sub(
                         'target: ([A-Z_]+|"[a-zA-Z:_-]+"),',
                        f'target: "{target}",',
                        line
                    )

                    new_line += f"\n{i}: {line}"
                else:
                    # Multi-line logs with no target set
                    # Insert an extra line with the target
                    leading_space = lambda line: len(line) - len(line.lstrip())

                    added_line = (" "*leading_space(line)
                                  + f'target: "{target}",')
                    result += f"{added_line}\n"

                    new_line += f"\n{i}: {added_line}\n{i + 1}: {line}"

                line_modified = True

        if line_modified:
            assert old_line is not None and new_line is not None
            print(
                Fore.RED
                + textwrap.indent(old_line, "    < ")
                + Style.RESET_ALL
            )
            print(
                Fore.GREEN
                + textwrap.indent(new_line, "    > ")
                + Style.RESET_ALL
            )
            print()

        result += f"{line}\n"
        i += 1
    return result

def main():
    for fname in glob.glob("**/*.rs", root_dir="src/", recursive=True):
        with open(f"src/{fname}", "r") as f:
            contents = f.read()

        contents = replace(fname, contents)

        # Doesn't write anything yet
        #with open(fname, "w") as f:
        #    f.write(contents)

if __name__ == "__main__":
    main()

