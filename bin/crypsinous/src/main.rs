use darkfi::{
    stakeholder::Stakeholder,
    blockchain::{EpochConsensus}
};

fn main()
{
    let slots=3;
    let reward=22;
    let epoch_consensus = EpochConsensus::new(slots, reward);
    /// read n from the cmd
    let n = 3;
    /// initialize n stakeholders
    let stakeholders = vec!(n);
    for i in n {
        let stakeholder = Stakeholder::new();
        stakeholders.push(stakeholder);
    }
    /// when the clock signal a new slot.
    /// check for leadership.
    /// if lead publish construct block metadata.
    /// push the new block before the end of the slot (clock should siganl the beging, and 1/k of the way to the end).
    ///TODO stakeholder should signal new epoch, new slot in the background
}
