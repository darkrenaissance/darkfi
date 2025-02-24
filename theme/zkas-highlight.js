hljs.registerLanguage("zkas", function (hljs) {
  return {
    name: "zkas",
    case_insensitive: false,
    keywords: {
      keyword: "k field constant witness circuit",
      literal: "true false VALUE_COMMIT_VALUE VALUE_COMMIT_RANDOM NULLIFIER_K",
      type:
        "EcPoint EcFixedPoint EcFixedPointBase EcFixedPointShort " +
        "EcNiPoint Base BaseArray Scalar ScalarArray MerklePath Uint32 Uint64",
      built_in:
        "ec_add ec_mul ec_mul_base ec_mul_short ec_mul_var_base " +
        "ec_get_x ec_get_y base_add base_mul base_sub poseidon_hash " +
        "merkle_root range_check less_than_strict less_than_loose bool_check " +
        "cond_select zero_cond witness_base constrain_equal_base " +
        "constrain_equal_point constrain_instance debug",
    },
    contains: [
      hljs.COMMENT("#", "$"),
      hljs.QUOTE_STRING_MODE,
      hljs.NUMBER_MODE,
    ],
  };
});
hljs.initHighlightingOnLoad();
