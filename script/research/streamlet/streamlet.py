from tinysmpc import VirtualMachine, PrivateScalar, SharedScalar

# Generating generals
general0 = VirtualMachine('general0')
general1 = VirtualMachine('general1')
general2 = VirtualMachine('general2')
general3 = VirtualMachine('general3')

# Using a simple number to represent the block for testing purposes
print('General 0 is the leader and shares block 42...')
block = PrivateScalar(42, general0)
shared_block = block.share([general0, general1, general2, general3])
print(general0)
print(general1)
print(general2)
print(general3)
print()

# 1 Stands for vote for, 0 for vote against
print('Generals vote on the block...')
general0_vote = PrivateScalar(1, general0)
general1_vote = PrivateScalar(1, general1)
general2_vote = PrivateScalar(1, general2)
general3_vote = PrivateScalar(0, general3)
shared_general0_vote = general0_vote.share([general0, general1, general2, general3])
shared_general1_vote = general1_vote.share([general0, general1, general2, general3])
shared_general2_vote = general2_vote.share([general0, general1, general2, general3])
shared_general3_vote = general3_vote.share([general0, general1, general2, general3])
print(general0)
print(general1)
print(general2)
print(general3)
print()

# Each general sums votes to notarize block if votes exceed 2n/3
print('Generals check votes...')
votes_thresshold = (2*4)/3
generals_votes_sum = shared_general0_vote + shared_general1_vote + shared_general2_vote + shared_general3_vote

general0_votes_sum = generals_votes_sum.reconstruct(general0)
print('General 0 votes sum: {0}'.format(general0_votes_sum.value))
if (general0_votes_sum.value > votes_thresshold):
    print('General 0 will notarize block')
else:
    print('General 0 will not notarize block')
       
general1_votes_sum = generals_votes_sum.reconstruct(general1)
print('General 1 votes sum: {0}'.format(general1_votes_sum.value))
if (general1_votes_sum.value > votes_thresshold):
    print('General 1 will notarize block')
else:
    print('General 1 will not notarize block')

general2_votes_sum = generals_votes_sum.reconstruct(general2)
print('General 2 votes sum: {0}'.format(general2_votes_sum.value))
if (general2_votes_sum.value > votes_thresshold):
    print('General 2 will notarize block')
else:
    print('General 2 will not notarize block')

general3_votes_sum = generals_votes_sum.reconstruct(general3)
print('General 3 votes sum: {0}'.format(general3_votes_sum.value))
if (general3_votes_sum.value > votes_thresshold):
    print('General 3 will notarize block')
else:
    print('General 3 will not notarize block')
