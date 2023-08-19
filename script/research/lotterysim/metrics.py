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
