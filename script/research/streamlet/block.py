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

class Block:
	''' This class represents a tuple of the form (h, e, txs).
		Each blocks parent hash h may be computed simply as a hash of the parent block. '''

	def __init__(self, h, e, txs):
		self.h = h	# parent hash
		self.e = e	# epoch number
		self.txs = txs	# transactions payload
		self.votes = []	 # Epoch votes
		self.notarized = False	# block notarization flag
		self.finalized = False	# block finalization flag

	def __repr__(self):
		return "Block=[h={0}, e={1}, txs={2}, notarized={3}, finalized={4}]".format(
			self.h, self.e, self.txs, self.notarized, self.finalized)

	def __hash__(self):
		# python hash is used for demostranation porpuses only.
		return hash((self.h, self.e, str(self.txs)))

	def __eq__(self, other):
		return self.h == other.h and self.e == other.e and self.txs == other.txs

	def encode(self):
		return(("{0},{1},{2}".format(self.h, self.e, self.txs)).encode())
