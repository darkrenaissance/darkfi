# Areas of work
There are several areas of work that are either undergoing maintenance 
or need to be maintained:

**Documentation:** general documentation and code docs (cargo doc). this is a very 
important work for example [overview](https://darkrenaissance.github.io/darkfi/architecture/overview.html) 
page is out of date.

**Tooling:** Such as the `drk` tool. right now 
we're adding [DAO functionality](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/dao/wallet.sql) 
to it.

**Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.

**Cleanup:** General code cleanup. for example flattening headers and improving things like in 
[this commit](https://github.com/darkrenaissance/darkfi/commit/9cd9c3113eed1b5f0bcad2ee449ef926d0908d55).

**ZK Debugger:** The ZKVM needs a debugger so we can interactively inspect values 
at each step to see where problems go wrong.

**ZK Special Tool:** We need a special tool to run zk contracts, where you can create 
a json file with the input values and public values, then run the zk 
contract without having to write any rust code. so you can write .zk 
files and try them out without having to write rust code. It will tell 
you the time to create and verify the proof, as well as the byte size of 
the proof.

**Events System:** We need to fix IRCD, we will need to implement the 
[events](https://darkrenaissance.github.io/darkfi/misc/event_graph/event_graph.html) system.
