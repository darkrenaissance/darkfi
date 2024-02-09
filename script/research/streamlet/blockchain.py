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

class Blockchain:
	''' This class represents a sequence of blocks starting with the genesis block. '''

	def __init__(self, intial_block):
		self.blocks = [intial_block]

	def __repr__(self):
		return "Blockchain=[blocks={0}]".format(self.blocks)

	def __eq__(self, other):
		return self.blocks == other.blocks

	def __len__(self):
		return len(self.blocks)

	def __getitem__(self, index):
		return self.blocks[index]

	def check_block_validity(self, block, previous_block):
		''' A block is considered valid when its parent hash is equal to the hash of the
			previous block and their epochs are incremental, exluding genesis.
			Aadditional validity rules can be applied. '''

		assert(block.h != '⊥')	# genesis block check
		assert(block.h == hash(previous_block))
		assert(block.e > previous_block.e)

	def check_chain_validity(self):
		''' A blockchain is considered valid, when every block is valid, based on check_block_validity method. '''
		
		for index, block in enumerate(self.blocks[1:]):
			self.check_block_validity(block, self.blocks[index])

	def add_block(self, block):
		''' Insertion of a valid block. '''
		
		self.check_block_validity(block, self.blocks[-1])
		self.blocks.append(block)

	def is_notarized(self):
		''' Blockchain notarization check. '''
		
		for block in self.blocks:
			if not block.notarized:
				return False
		return True
