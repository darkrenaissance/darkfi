# Token Id

each token has unique token id derived as:
$$ hash(PREFIX || key^{public}_x || key^{public}_y) $$
`key` is authority key, or public key.

%# validate unique id
%validate newly minted tokens doesn't match any token mint transaction's token Id.
