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

# Section 3.3 from "Streamlet: Textbook Streamlined Blockchains"

from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import rsa, padding
from cryptography.hazmat.backends import default_backend
from cryptography.exceptions import InvalidSignature

# Cryptographic algorithm used is for demostranation porpuses only.
# Generating the keys pair. 
def generate_keys(private_key_password):
	private_key = rsa.generate_private_key(
		public_exponent=65537,
		key_size=2048
	)	
	encrypted_pem_private_key = private_key.private_bytes(
		encoding=serialization.Encoding.PEM,
		format=serialization.PrivateFormat.PKCS8,
		encryption_algorithm=serialization.BestAvailableEncryption(private_key_password.encode())
	)
	pem_public_key = private_key.public_key().public_bytes(
	  encoding=serialization.Encoding.PEM,
	  format=serialization.PublicFormat.SubjectPublicKeyInfo
	)
	
	return encrypted_pem_private_key, pem_public_key

# Signs a message using private_key.
def sign_message(password, private_key, message):
	privkey = serialization.load_pem_private_key(private_key, password=password.encode(), backend=default_backend())
	signed_message = privkey.sign(
		message.encode(),
		padding.PSS(
			mgf=padding.MGF1(hashes.SHA256()),
			salt_length=padding.PSS.MAX_LENGTH),
		hashes.SHA256()
	)
	return signed_message
	
# Verifies a message against a public key.
def verify_signature(public_key, message, signed_message):
	pubkey = serialization.load_pem_public_key(public_key, backend=default_backend())
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

# When a node votes on a block, it simply signs it with the private key, and broadcasts the message to rest nodes.	
message = "block"
node_password = "node_password"	
node_private_key, node_public_key = generate_keys(node_password)
signed_message = sign_message(node_password, node_private_key, message)

# When nodes receive votes, they verify them against nodes public key.
assert(verify_signature(node_public_key, message, signed_message))
# If votes for that specific block are >=2n/3, node marks block as notarized.