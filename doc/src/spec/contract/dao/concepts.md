# Concepts

The governance process is divided in a few steps that are outlined below:

* **Propose:** a proposal is submitted to the blockchain.
* **Vote:** governance token holdens can vote on the proposal.
* **Exec:** if the proposal passes within the time limit then the proposal is
  executed.

## Propose

To prevent spam, proposals must be submitted by holders with a certain number
of governance tokens. Several members of the DAO can add inputs separately to
a proposal before it is posted on chain so that their combined inputs meet the
proposal governance token limit.

Once proposals are posted on chain, they are immediately active.

### Proposal States

* *Active*: the proposal is open to voting.
* *Expired*: the proposal passed its duration and can no longer be voted on.
* *Accepted*: the proposal gained sufficient votes but is not yet executed.
* *Executed*: the proposal was accepted and has been finalized on chain.

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

Quorum is defined as the minimum absolute number of governance tokens voting
yes required for a proposal to become accepted.

### Approval Ratio

The approval ratio is defined as the minimum proportion of yes votes for the
proposal to be accepted.

