# Contributing

## How to get started

Every monday 16:00 CET, there is our main dev meeting on
[our chat](https://darkrenaissance.github.io/darkfi/misc/ircd/ircd.html).
Feel free to join and discuss with other darkfi devs.

In general, the best way to get started is to explore the codebase thoroughly and
identify issues and areas of improvement.

Contribute according to your own interests, skills, and topics in which you would
like to become more knowledgable. Take initiative. Other darkfi devs can help you
as mentors: see [the Methodology section of the Study Guide](https://darkrenaissance.github.io/darkfi/development/learn.html#methodology).

Few people are able be an expert in all domains. Choose a topic and specialize.
Example specializations are described [here](https://darkrenaissance.github.io/darkfi/development/learn.html#branches).
Don't make the mistake that you must become an expert in all areas before getting started.
It's best to just jump in.

## Finding specific tasks

Tasks are usually noted in-line using code comments. All of these tasks should be resolved
and can be considered a priority.

To find them, run the following command:
```
git grep -E 'TODO|FIXME'
```

## Areas of work

There are several areas of work that are either undergoing maintenance 
or need to be maintained:

* **Documentation:** general documentation and code docs (cargo doc). This is a very 
  important work for example [overview](https://darkrenaissance.github.io/darkfi/architecture/overview.html) 
  page is out of date.
* **Tooling:** Such as the `drk` tool. right now 
  we're adding [DAO functionality](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/dao/wallet.sql) 
  to it.
* **Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.
* **Cleanup:** General code cleanup. for example flattening headers and improving things like in 
  [this commit](https://github.com/darkrenaissance/darkfi/commit/9cd9c3113eed1b5f0bcad2ee449ef926d0908d55).
* **ZK Debugger:** The ZKVM needs a debugger so we can interactively inspect values 
  at each step to see where problems go wrong.
* **ZK Special Tool:** We need a special tool to run zk contracts, where you can create 
  a json file with the input values and public values, then run the zk 
  contract without having to write any rust code. so you can write .zk 
  files and try them out without having to write rust code. It will tell 
  you the time to create and verify the proof, as well as the byte size of 
  the proof.
  * We should also have **Python bindings** for working with Scalars, EC points, merkle trees and hashing.
* **Events System:** We need to fix IRCD, we will need to implement the 
  [events](https://darkrenaissance.github.io/darkfi/misc/event_graph/event_graph.html) system.
