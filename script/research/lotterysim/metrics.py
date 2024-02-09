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

from core.constants import *
import config

def percent_change(start, end):
    difference = end - start
    change = (difference / start) * 100 if start!=0 else 0
    return change

#def slot_number_at_target(count, supply, target_reached):
#    if target_reached == False:
#        if supply > target:
#            target_reached = True
#            get_slot_time(count)
#    return target_reached
#
#target = ERC20DRK * config.exchange_rate
#
#def get_slot_time(count):
#    # @ 90 seconds per slot
#    slots_per_day = 960
#    slots_per_month = 28800
#    slots_per_year = slots_per_month * 12
#
#    days = count / slots_per_day
#    months = count / slots_per_month
#    years = count / slots_per_year
#
#    print("Target supply of", str(target), "DRK was achieved after: ",
#          str(count), "slots", str(days), "(D)", str(months), "(M)",
#          str(years), "(Y)")
