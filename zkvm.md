# do the setup for mint.zcd, save the params in mint.params
zkvm setup mint.zcd mint.setup
# make the proof
zkvm prove mint.zcd mint.setup mint-params.json proof.dat
# verify the proof
zkvm verify mint.zcd mint.setup proof.dat
# show public values in proof
zkvm public proof.dat

