use darkfi::{
    stakeholder::Stakeholder,
    blockchain::{EpochConsensus,},
    net::{Settings,},
};

use std::thread;

fn main()
{
    let slots=3;
    let reward=22;
    let epoch_consensus = EpochConsensus::new(slots, reward);
    /// read n from the cmd
    let n = 3;
    /// initialize n stakeholders
    let stakeholders = vec!(n);
    let settings = net::Settings::new();
    //TODO populate settings with peers urls
    let k : u32 = 13; //proof's number of rows
    let handles = vec!(0);
    for i in n {
        let stakeholder = Stakeholder::new(epoch_consensus, settings, Some(k));
        stakeholders.push(stakeholder);
        let handle = thread.spawn(|| {
            stakeholders.background();
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
}
