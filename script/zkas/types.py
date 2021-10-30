TYPE_ID_BASE                    = 0
TYPE_ID_SCALAR                  = 1
TYPE_ID_EC_POINT                = 2
TYPE_ID_EC_FIXED_POINT          = 3
TYPE_ID_MERKLE_PATH             = 4
# This is so we know the number of TYPE_ID stacks
TYPE_ID_LAST                    = 5

allowed_types = {
    "Base":                 TYPE_ID_BASE,
    "Scalar":               TYPE_ID_SCALAR,
    "EcFixedPoint":         TYPE_ID_EC_FIXED_POINT,
    "MerklePath":           TYPE_ID_MERKLE_PATH,
}
# Used for debug and error messages
type_id_to_name = dict((value, key) for key, value in allowed_types.items())

FUNC_ID_POSEIDON_HASH           = 0
FUNC_ID_ADD                     = 1
FUNC_ID_CONSTRAIN_INSTANCE      = 2
FUNC_ID_EC_MUL_SHORT            = 3
FUNC_ID_EC_MUL                  = 4
FUNC_ID_EC_ADD                  = 5
FUNC_ID_EC_GET_X                = 6
FUNC_ID_EC_GET_Y                = 7
FUNC_ID_CALCULATE_ROOT          = 8

class FuncFormat:

    def __init__(self, func_id, return_type_ids, param_types):
        self.func_id = func_id
        self.return_type_ids = return_type_ids
        self.param_types = param_types

    def total_arguments(self):
        return len(self.return_type_ids) + len(self.param_types)

function_formats = {
    "poseidon_hash": FuncFormat(
        # Funcion ID            Type ID             Parameter types
        FUNC_ID_POSEIDON_HASH,  [TYPE_ID_BASE],     [TYPE_ID_BASE,
                                                     TYPE_ID_BASE]
    ),
    "add": FuncFormat(
        FUNC_ID_ADD,            [TYPE_ID_BASE],     [TYPE_ID_BASE,
                                                     TYPE_ID_BASE]
    ),
    "constrain_instance": FuncFormat(
        FUNC_ID_CONSTRAIN_INSTANCE, [],             [TYPE_ID_BASE]
    ),
    "ec_mul_short": FuncFormat(
        FUNC_ID_EC_MUL_SHORT,   [TYPE_ID_EC_POINT], [TYPE_ID_BASE,
                                                     TYPE_ID_EC_FIXED_POINT]
    ),
    "ec_mul": FuncFormat(
        FUNC_ID_EC_MUL,         [TYPE_ID_EC_POINT], [TYPE_ID_SCALAR,
                                                     TYPE_ID_EC_FIXED_POINT]
    ),
    "ec_add": FuncFormat(
        FUNC_ID_EC_ADD,         [TYPE_ID_EC_POINT], [TYPE_ID_EC_POINT,
                                                     TYPE_ID_EC_POINT]
    ),
    "ec_get_x": FuncFormat(
        FUNC_ID_EC_GET_X,       [TYPE_ID_BASE],     [TYPE_ID_EC_POINT]
    ),
    "ec_get_y": FuncFormat(
        FUNC_ID_EC_GET_Y,       [TYPE_ID_BASE],     [TYPE_ID_EC_POINT]
    ),
    "calculate_root": FuncFormat(
        FUNC_ID_CALCULATE_ROOT, [TYPE_ID_BASE],     [TYPE_ID_MERKLE_PATH,
                                                     TYPE_ID_BASE]
    ),
}

func_id_to_name = dict((fmt.func_id, key) for key, fmt
                       in function_formats.items())
