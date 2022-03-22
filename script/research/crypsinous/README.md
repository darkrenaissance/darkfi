this is an effort to break down the building blocks of crypsinous blockchain

# Crypsinous blockchain
Each part $U_p$ stores it's own local view of the Blockchain $C_{loc}^{U_p}$.
$C_{loc}$ is a sequence of blocks $B_i$ (i>0), where each $B \in C_{loc}$
$$ B = (tx_{lead},st)$$
$$tx_{lead} = (LEAD,st\overrightarrow{x}_{ref},stx_{proof})$$
$st\overrightarrow{x}_{ref}$ it's a vector of $tx_{lead}$ that aren't yet in $C_{loc}$.
$stx_{proof}=(cm_{\prime{c}},sn_c,ep,sl,\rho,h,ptr,\pi)$
the Blocks' $\emph{st}$ is the block data, and $\emph{h}$ is the hash of that data.
the commitment of the newly created coin is:
$(cm_{\prime{c}},r_{\prime{c}})=COMM(pk^{COIN}||\tau||v_c||\rho_{\prime{c}})$,
\emph{$sn_c$} is the coin's serial number revealed to spend the coin.
$$sn_c=PRF_{root_{sk}^{COIN}}^{sn}(\rho_c)$$
$$\rho=\eta^{sk_{sl}^{COIN}}$$
$\eta$ is is from random oracle evaluated at $(Nonce||\eta_{ep}||sl)$, $\rho$ is the following epoch's seed. $\emph{ptr}$ is the hash of the previous block, $\pi$ is the NIZK proof of the LEAD statement.

## LEAD statement
for $x=(cm_{c_2},sn_{c_1},\eta,sl,\rho,h,ptr,\mu_{\rho},\mu_{y},root)$, and
$w=(path,root_{sk^{COIN}},path_{sk^{COIN}},\tau_c,\rho_c,r_{c_1},v,r_{c_2})$
for tuple $(x,w) \in L_{lead}$ iff:

 * $pk^{COIN} = RPF_{root_{sk^{COIN}}}^{pk}(\tau_c)$.
 * $\rho_{c_2}=RPF_{root_{sk_{c_1}^{COIN}}}^{evl}(\rho_{c_1})$.
 * $\forall i \in \{1,2\} : DeComm(cm_{c_i},pk^{COIN}||v||\rho_{c_i},r_{c_i})=T$.
 * \emph{path} is a valid Merkle tree path to cm_c_1 in the tree with the root \emph{root}.
 * \emph{$path_{sk^{COIN}}$} is a valid path to a leaf at position $sl-\tau_c$ in a tree with a root $root_{sk^{COIN}}$.
 * $sn_{c_1}= RPF_{root_{sk}^{COIN}}^{sn}(\rho_{c_1})$
 * $y = \mu_{y}^{root_{sk_{c_1}^{COIN}}||\rho_c}$
 * $\rho = \mu_{\rho}^{root_{sk_{c_1}^{COIN}}||\rho_c}$
 * $y< ord(G)\phi_f(v)$
