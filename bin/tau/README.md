

# Tau

Tasks management app using peer-to-peer network and raft consensus.  
multiple users can collaborate by working on the same tasks, and all users will have synced task list.


## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi/bin/tau
% make 
% make install  
```

## Usage

```shell
% tau --help 
```

	tau 0.3.0
	Tau cli
	
	USAGE:
	    tau [OPTIONS] [SUBCOMMAND]
	
	OPTIONS:
	    -h, --help               Print help information
	        --listen <LISTEN>    Rpc address [default: 127.0.0.1:8875]
	    -v                       Increase verbosity
	    -V, --version            Print version information
	
	SUBCOMMANDS:
	    add            Add a new task
	    get-comment    Get task's comments
	    get-state      Get task state
	    help           Print this message or the help of the given subcommand(s)
	    list           List open tasks
	    set-comment    Set comment for a task
	    set-state      Set task state
	    show           Show task by ID
	    update         Update/Edit an existing task by ID


