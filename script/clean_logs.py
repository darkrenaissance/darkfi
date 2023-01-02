import glob, re, os.path, textwrap
from colorama import Fore, Back, Style

def target_prefix(dir):
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

def target_suffix(prefix, base):
    if prefix in ("dao", "money"):
        # Just shorten the target to simply "dao" or "money"
        # We don't need the fine grained details
        return ""
    elif base in ("mod.rs", "lib.rs"):
        # Just use the module name as the target with these files
        return ""
    # Otherwise just use the filename as the target suffix
    return base.removesuffix(".rs")

def log_target(fname):
    dir, base = os.path.dirname(fname), os.path.basename(fname)
    prefix = target_prefix(dir)
    suffix = target_suffix(prefix, base)
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

    result = []
    lines = contents.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        # Line number
        ln = lambda: i + 1

        # only used for debug output
        old_text = None
        new_text = None
        # This is used as a debug goto
        is_modified = False

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
            # Walk backwards to find the function name
            # Range is (i, 0]
            function_name = None
            for j in range(i - 1, -1, -1):
                past_line = lines[j]
                if (match := re.search(
                    f"fn ([a-zA-Z0-9_]+)(<[a-zA-Z: +']+>)?\\(",
                    past_line
                )):
                    function_name = match.group(1)
                    break
            assert function_name is not None

            if target in (
                "consensus::protocol_proposal",
                "consensus::protocol_sync",
                "consensus::protocol_sync_consensus",
                "consensus::protocol_tx",
                "runtime::db",
                "net::hosts",
                "net::protocol_address",
                "net::protocol_ping",
                "net::protocol_seed",
                "net::protocol_version",
                "net::p2p",
            ):
                target += f"::{function_name}()"

            # No target exists for this file at all. Just ignore these
            # We would normally delete any target set for these files
            # but so far we have none of them, so just ignore them.
            if not target:
                print(
                    "    "
                    + Back.RED + "Skip [no target]:" + Style.RESET_ALL
                    + f" {line}"
                )
            # Single lines with a target that's a constant or string
            elif re.search(rf'{log_level}!\(target: ([\w]+|"[\w:\-\(\)]+"),', line):
                old_text = f"{ln()}: {line}"

                line = re.sub(
                     'target: ([A-Z_]+|"[a-zA-Z:_-]+"),',
                    f'target: "{target}",',
                    line
                )

                is_modified = True
                new_text = f"{ln()}: {line}"
            # Normal single lines with no target set
            elif f'{log_level}!("' in line:
                old_text = f"{ln()}: {line}"

                #print(f"    No target: {line}")
                line = line.replace(f'{log_level}!(',
                                    f'{log_level}!(target: "{target}", ')

                is_modified = True
                new_text = f"{ln()}: {line}"
            # Multiline logs
            # We read the next line and check if there's a target set or not
            else:
                assert re.search(rf"{log_level}!\($", line)

                old_text = f"{ln()}: {line}"
                new_text = f"{ln()}: {line}"

                result.append(line)
                i += 1
                assert i < len(lines)
                line = lines[i]

                old_text += f"\n{ln()}: {line}"

                # Constants or target strings set
                if re.search(r'target: ([\w]+|"[\w:\-\(\)]+"),', line):
                    line = re.sub(
                        r'target: ([\w]+|"[\w:\-\(\)]+"),',
                        f'target: "{target}",',
                        line
                    )

                    new_text += f"\n{ln()}: {line}"
                # Multi-line logs with no target set
                # Insert an extra line with the target
                else:
                    assert re.search('^"', line)

                    leading_space = lambda line: len(line) - len(line.lstrip())

                    added_line = (" "*leading_space(line)
                                  + f'target: "{target}",')
                    result.append(added_line)

                    new_text += f"\n{ln()}: {added_line}\n{ln() + 1}: {line}"

                is_modified = True

        if is_modified:
            assert old_text is not None and new_text is not None
            print(
                Fore.RED
                + textwrap.indent(old_text, "    < ")
                + Style.RESET_ALL
            )
            print(
                Fore.GREEN
                + textwrap.indent(new_text, "    > ")
                + Style.RESET_ALL
            )
            print()

        result.append(line)
        i += 1
    return "\n".join(result)

def main():
    for fname in glob.glob("**/*.rs", root_dir="src/", recursive=True):
        with open(f"src/{fname}", "r") as f:
            contents = f.read()

        contents = replace(fname, contents)

        # Uncomment this to apply the changes
        #with open(f"src/{fname}", "w") as f:
        #    f.write(contents)

if __name__ == "__main__":
    main()

