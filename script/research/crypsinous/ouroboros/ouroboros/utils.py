/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import rsa, padding
from cryptography.hazmat.backends import default_backend
from cryptography.exceptions import InvalidSignature
import random
import pickle

def extended_euclidean_algorithm(a, b):
    """
    Returns a three-tuple (gcd, x, y) such that
    a * x + b * y == gcd, where gcd is the greatest
    common divisor of a and b.

    This function implements the extended Euclidean
    algorithm and runs in O(log b) in the worst case.
    """
    s, old_s = 0, 1
    t, old_t = 1, 0
    r, old_r = b, a

    while r != 0:
        quotient = old_r // r
        old_r, r = r, old_r - quotient * r
        old_s, s = s, old_s - quotient * s
        old_t, t = t, old_t - quotient * t

    return old_r, old_s, old_t


def inverse_of(n, p):
    """
    Returns the multiplicative inverse of
    n modulo p.

    This function returns an integer m such that
    (n * m) % p == 1.
    """
    gcd, x, y = extended_euclidean_algorithm(n, p)
    assert (n * x + p * y) % p == gcd

    if gcd != 1:
        # Either n is 0, or p is not a prime number.
        raise ValueError(
            '{} has no multiplicative inverse '
            'modulo {}'.format(n, p))
    else:
        return x % p

'''
@param nums: list of weight
@param true_rnd_fn: truely random function
@return zero-based index of the truely selected element
'''
def weighted_random(nums, true_rnd_fn=random.random):
    """
    nums is list of weight, it return the truely random
    weighted value.
    """
    L = len(nums)
    pair = [(i, nums[i]) for i in range(L)]
    pair.sort(key=lambda p: p[1])
    tot = sum([pair[i][1] for i in range(L)])
    frequency = [pair[i][1]/tot for i in range(L)]
    acc_prop = [sum(frequency[:i+1]) for i in range(L)]
    rnd = true_rnd_fn()
    for elected in range(L):
        if rnd<=acc_prop[elected]:
            break
    return pair[elected][0]


'''
@param data: data is dictionary of  list of (pk_i, s_i) public key,
        and stake respectively of the corresponding stakeholder U_i,
        seed of the leader election function.
'''
def encode_genesis_data(data):
    return pickle.dumps(data)

def decode_gensis_data(encoded_data):
    return pickle.loads(encoded_data)

'''
TODO this is a  adhoc solution
this has is used to compute the state of block from the previous block
'''
def state_hash(obj):
    return hash(obj)


'''
TODO this is a  adhoc solution
this is used to generate VRF's sk from some seed
note there is a need for nounce to be concatenated with the seed,
just in case two stakeholders started with the same seed
(for the time being the seed is provided by the stakeholder, it's stakeholder passowrd)
'''
def vrf_hash(seed):
    return hash(seed)


def generate_sig_keys(private_key_password):
	''' Generating the keys pair. Cryptographic algorithm used is for demostranation porpuses only. '''
	
	private_key = rsa.generate_private_key(
		public_exponent=65537,
		key_size=2048
	)
	encrypted_pem_private_key = private_key.private_bytes(
		encoding=serialization.Encoding.PEM,
		format=serialization.PrivateFormat.PKCS8,
		encryption_algorithm=serialization.BestAvailableEncryption(
			private_key_password.encode()))
	pem_public_key = private_key.public_key().public_bytes(
		encoding=serialization.Encoding.PEM,
		format=serialization.PublicFormat.SubjectPublicKeyInfo
	)

	return encrypted_pem_private_key, pem_public_key

def sign_message(password, private_key, message):
	''' Signs a message using private_key. '''
	
	privkey = serialization.load_pem_private_key(
		private_key, password=password.encode(), backend=default_backend())
	signed_message = privkey.sign(
		message.encode(),
		padding.PSS(
			mgf=padding.MGF1(hashes.SHA256()),
			salt_length=padding.PSS.MAX_LENGTH),
		hashes.SHA256()
	)
	return signed_message

def verify_signature(public_key, message, signed_message):
	''' Verifies a message against a public key. '''

	pubkey = serialization.load_pem_public_key(
		public_key, backend=default_backend())
	try:
		pubkey.verify(
			signed_message,
			message.encode(),
			padding.PSS(
				mgf=padding.MGF1(hashes.SHA256()),
				salt_length=padding.PSS.MAX_LENGTH),
			hashes.SHA256())
		return True
	except InvalidSignature:
		return False
