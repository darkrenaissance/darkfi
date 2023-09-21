load('../mpc/share.sage')

def countZeros(x):
    total_bits = 32
    res = 0
    count = 0
    while ((x & (1 << (total_bits - 1))) == 0) and count < 32:
        x = (x << 1)
        res += 1
        count += 1
    return res

def sum_shares(shares, source, party_id):
    zero_share = AuthenticatedShare(0, source, party_id)
    for share in shares:
        zero_share += share
    return zero_share

def sum_ec_shares(shares):
    zero_share = ECAuthenticatedShare(0)
    for share in shares:
        zero_share += share
    return zero_share

def shares_mul(my_shares, peer_shares):
    return sum([my_share * peer_share for my_share, peer_share in zip(my_shares, peer_shares)])

def to_ec_shares(ec):
    return ECAuthenticatedShare(ec)

def to_ec_shares_list (ec_list):
    return [to_ec_shares(ec) for ec in ec_list]
