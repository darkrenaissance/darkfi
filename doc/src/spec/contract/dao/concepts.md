# Concepts

The governance process is divided in a few steps that are outlined below:

* **Propose:** a proposal is submitted to the blockchain.
* **Vote:** governance token holdens can vote on the proposal.
* **Exec:** if the proposal passes within the time limit then the proposal is
  executed.

> Note:
> There is a special case where a proposal can be executed before voting period
> passes, if its strongly supported, based on the configured early execution quorum.

## Propose

To prevent spam, proposals must be submitted by holders with a certain number
of governance tokens and the DAO proposers key. Several members of the DAO can
add inputs separately to a proposal before it is posted on chain so that their
combined inputs meet the proposal governance token limit.

Once proposals are posted on chain, they are immediately active.

### Proposal States

* *Active*: the proposal is open to voting.
* *Expired*: the proposal passed its duration and can no longer be voted on.
* *Accepted*: the proposal gained sufficient votes but is not yet executed.
* *Executed*: the proposal was accepted and has been confirmed on chain.

## Vote

### Participants

*Participants* are users that have the right to vote on proposals. Participants
are holders of governance tokens specified in the DAO.

Note that only participants from before the proposal is submitted on chain
are eligible to vote. That means receivers of governance tokens after a proposal is
submitted will *not* be eligible to vote.

There are currently two voting options:

* Yes
* No

### Voting Period

Once a proposal passes its duration, which is measured in 4 hour block windows, participants can
no longer vote on the proposal, and it is considered *expired*.

### Quorum

Quorum is defined as the minimal threshold of participating total governance tokens need for
a proposal to pass. Normally this is implemented as min % of voting power, but we do this in
absolute value.

### Early Execution Quorum
Early execytuib quorum is defined as the minimal threshold of participating total tokens needed
for a proposal to be considered as strongly supported, enabling early execution. Must be greater
or equal to normal quorum.

### Approval Ratio

The approval ratio is defined as the minimum proportion of yes votes for the
proposal to be accepted.

