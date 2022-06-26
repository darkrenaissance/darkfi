#!/usr/bin/env python3
import numpy
from finite_fields.finitefield import IntegersModP

round_constants = [
    [
        0x360d7470611e473d353f628f76d110f34e71162f31003b7057538c2596426303,
        0x2bab94d7ae222d135dc3c6c5febfaa314908ac2f12ebe06fbdb74213bf63188b,
        0x150c93fef652fb1c2bf03e1a29aa871fef77e7d736766c5d0939d92753cc5dc8
    ],
    [
        0x3270661e68928b3a955d55db56dc57c103cc0a60141e894e14259dce537782b2,
        0x073f116f04122e25a0b7afe4e2057299b407c370f2b5a1ccce9fb9ffc345afb3,
        0x2a32ec5c4ee5b1837affd09c1f53f5fd55c9cd2061ae93ca8ebad76fc71554d8
    ],
    [
        0x270326ee039df19e651e2cfc740628ca634d24fc6e2559f22d8ccbe292efeead,
        0x27c6642ac633bc66dc100fe7fcfa54918af895bce012f182a068fc37c182e274,
        0x1bdfd8b01401c70ad27f57396989129d710e1fb6ab976a459ca18682e26d7ff9
    ],
    [
        0x162a14c62f9a89b814b9d6a9c84dd678f4f6fb3f9054d373c832d824261a35ea,
        0x2d193e0f76de586b2af6f79e3127feeaac0a1fc71e2cf0c0f79824667b5b6bec,
        0x044ca3cc4a85d73b81696ef1104e674f4feff82984990ff85d0bf58dc8a4aa94
    ],
    [
        0x1cbaf2b371dac6a81d0453416d3e235cb8d9e2d4f314f46f6198785f0cd6b9af,
        0x1d5b2777692c205b0e6c49d061b6b5f4293c4ab038fdbbdc343e07610f3fede5,
        0x2e9bdbba3dd34bffaa30535bdd749a7e06a9adb0c1e6f962f60e971b8d73b04f
    ],
    [
        0x2de11886b18011ca8bd5bae36969299fde40fbe26d047b05035a13661f22418b,
        0x2e07de1780b8a70d0d5b4a3f1841dcd82ab9395c449be947bc998884ba96a721,
        0x0f69f1854d20ca0cbbdb63dbd52dad16250440a99d6b8af3825e4c2bb74925ca
    ],
    [
        0x2eb1b25417fe17670d135dc639fb09a46ce5113507f96de9816c059422dc705e,
        0x115cd0a0643cfb988c24cb44c3fab48aff36c661d26cc42db8b1bdf4953bd82c,
        0x26ca293f7b2c462d066d7378b999868bbb57ddf14e0f958ade801612311d04cd
    ],
    [
        0x17bf1b93c4c7e01a2a830aa162412cd90f160bf9f71e967ff5209d14b24820ca,
        0x35b41a7ac4f3c571a24f8456369c85dfe03c0354bd8cfd3805c86f2e7dc293c5,
        0x3b1480080523c439435927994849bea964e14d3beb2dddde72ac156af435d09e
    ],
    [
        0x2cc6810031dc1b0d4950856dc907d57508e286442a2d3eb2271618d874b14c6d,
        0x25bdbbeda1bde8c1059618e2afd2ef999e517aa93b78341d91f318c09f0cb566,
        0x392a4a8758e06ee8b95f33c25dde8ac02a5ed0a27b61926cc6313487073f7f7b
    ],
    [
        0x272a55878a08442b9aa6111f4de009485e6a6fd15db89365e7bbcef02eb5866c,
        0x2d5b308b0cf02cdfefa13c4e60e26239a6ebba011694dd129b925b3c5b21e0e2,
        0x16549fc6af2f3b72dd5d293d72e2e5f244dff42f18b46c56ef38c57c311673ac
    ],
    [
        0x1b10bb7a82afce39fa69c3a2ad52f76d76398265344203119b7126d9b46860df,
        0x0f1e7505ebd91d2fc79c2df7dc98a3bed1b36968ba0405c090d27f6a00b7dfc8,
        0x2f313faf0d3f6187537a7497a3b43f46797fd6e3f18eb1caff457756b819bb20
    ],
    [
        0x3a5cbb6de450b481fa3ca61c0ed15bc55cad11ebf0f7ceb8f0bc3e732ecb26f6,
        0x3dab54bc9bef688dd92086e253b439d651baa6e20f892b62865527cbca915982,
        0x06dbfb42b979884de280d31670123f744c24b33b410fefd4368045acf2b71ae3
    ],
    [
        0x068d6b4608aae810c6f039ea1973a63eb8d2de72e3d2c9eca7fc32d22f18b9d3,
        0x366ebfafa3ad381c0ee258c9b8fdfccdb868a7d7e1f1f69a2b5dfcc5572555df,
        0x39678f65512f1ee404db3024f41d3f567ef66d89d044d022e6bc229e95bc76b1
    ],
    [
        0x21668f016a8063c0d58b7750a3bc2fe1cf82c25f99dc01a4e534c88fe53d85fe,
        0x39d00994a8a5046a1bc749363e98a768e34dea56439fe1954bef429bc5331608,
        0x1f9dbdc3f84312636b203bbe12fb3425b163d41605d39f99770c956f60d881b3
    ],
    [
        0x027745a9cddfad95e5f17b9e0ee0cab6be0bc829fe5e66c69794a9f7c336eab2,
        0x1cec0803c504b635788d695c61e932122fa43fe20a45c78d52025657abd8aee0,
        0x123523d75e9fabc172077448ef87cc6eed5082c8dbf31365d3872a9559a03a73
    ],
    [
        0x1723d1452c9cf02df419b848e5d694bf27feba35975ee7e5001779e3a1d357f4,
        0x1739d180a16010bdfcc0573d7e61369421c3f776f572836d9dab1ee4dcf96622,
        0x2d4e6354da9cc554acce32391794b627fafa96fbeb0ab89370290452042d048d
    ],
    [
        0x153ee6142e535e334a869553c9d007f88f3bd43f99260621670bcf6f8b485dcd,
        0x0c45bfd3a69aaa65635ef7e7a430b486968ad4424af83700d258d2e2b7782172,
        0x0adfd53b256a6957f2d56aec831446006897ac0a8ffa5ff10e5633d251f73307
    ],
    [
        0x315d2ac8ebdbac3c8cd1726b7cbab8ee3f87b28f1c1be4bdac9d36a8b7516d63,
        0x1b8472712d02eef4cfaec23d2b16883fc9bb60d1f6959879299ce44ea423d8e1,
        0x3c1cd07efda6ff24bd0b70fa2255eb6f367d2c54e36928c9c4a5404198adf70c
    ],
    [
        0x136052d26bb3d373687f4e51b2e1dcd34a16073f738f7e0cbbe523aef9ab107a,
        0x16c96beef6a0a848c1bdd859a1232a1d7b3cfbb873032681676c36c24ef967dd,
        0x284b38c57ff65c262ab7fed8f499a9fb012387bab4f1662d067eec7f2d6340c4
    ],
    [
        0x0c5993d175e81f6639e242198897d17cfc06772c1c0411a6af1dff204c922f86,
        0x03bf7a3f7bd043dafcda655d1ba9c8f9f24887ad48e17759bbf53f67b1f87b15,
        0x3188fe4ee9f9fafbb0cf999567f00e734c8f9cbe69f0e8279b5cd09e36d8be62
    ],
    [
        0x171f528ccf6584375a39768c480d61e13af5bf77c1c42652afea99a2ec6c595a,
        0x12f4175c4ab45afc196e41859b35ef88812c3286ee7000675a0563b9b8e9f1d5,
        0x3a509e155cb7ebfd8f8fdcf800a9ac697e23e1aabe96cfab0e74d4d369118b79
    ],
    [
        0x10f2a685df4a27c81a89920e2504c3b3984bc8f2e4c1b69e98712c65678cfd30,
        0x09e5f49790c8a0e21d8d93d54ab91a0e54573c9333c56321e8a16728cc9d4918,
        0x352d69bed80ee3e52bf35705d9f84a3442d17ed6ee0fab7e609a740347cf5fea
    ],
    [
        0x058ee73ba9f3f293491562faf2b190d3c634debd281b76a63a758af6fa84e0e8,
        0x232f99cc911eddd9cd0f1fc55b1a3250092cb92119bc76be621a132510a43904,
        0x201beed7b8f3ab8186c22c6c5d4869f0f9efd52ca6bc2961c3b97c1e301bc213
    ],
    [
        0x1376dce6580030c6a1c9291d58602f5129388842744a1210bf6b3431ba94e9bc,
        0x1793199e6fd6ba342b3356c38238f761072ba8b02d92e7226454843c5486d7b3,
        0x22de7a7488dcc7359fee9c20c87a67df3c66160dc62aacac06a3f1d3b433311b
    ],
    [
        0x3514d5e9066bb160df8ff37fe2d8edf8dbe0b77fae77e1d030d6e3fd516b47a8,
        0x30cd3006931ad636f919a00dabbf5fa5ff453d6f900f144a19377427137a81c7,
        0x253d1a5c5293412741f81a5cf613c8df8f9e4b2cae2ebb515b6a74220692b506
    ],
    [
        0x035b461c02d79d19a35e9613e7f5fe92851b3a59c990fafc73f666cb86a48e8e,
        0x23a9928079d175bd5bc00eedd56b93e092b1283c2d5fccde7cfbf86a3aa04780,
        0x13a7785ae134ea92f1594a0763c611abb5e2ea3436eef957f1e4ccd73fa00a82
    ],
    [
        0x39fce308b7d43c574962ae3c0da17e313889c57863446d88bbf04f5252de4279,
        0x1aae18833f8e1d3ac0fdf01662f60d22bef00a08c6ed38d23b57e34489b53fad,
        0x1a761ce82400af018b2e80c064fd83ed27c1b3fd8f85d8a855513e033398513f
    ],
    [
        0x275a03e45adda7c316dd1a87ca22e1ccdcf6af2830a502875244ca749b73e481,
        0x2e5a10f08b5ab8bbeb08e47e5feabcf807e561453fc5648b58a253cfb6a95786,
        0x1459cb8587208473b84e9c333b2932f1c141a5b6d594bec4e033d82cefe78ce3
    ],
    [
        0x193ae5921d78b5de7b92ce810e14a40052f9332fbffcfbbd5cec7e7b338fbe1b,
        0x3097898a5d0011a489111fb2c4660281374384f4a072820560224be67248e82c,
        0x378d97bf8c864ae7571782fd96ce54b41979b2d1c465b4d9549980de862930f5
    ],
    [
        0x2eb04ea7c01d97ec88136287ce376b08dbc7f5cb4609342137ea32a971d17884,
        0x36425347ea03f6412302a1c22e49baec861cbda476804e6cead3726f1af2e7b0,
        0x26b72df47408ad42cc996cd85c98a1d83f5b5ca5a19a9701ecd627e59590d09e
    ],
    [
        0x130180e44e2924db1f05636c610b89aade01212ee4588f8959bece31f0a31e95,
        0x219e97737d3979ba73275acaed5f579cdf7793cc89e5b52f9ea8e7bc79263550,
        0x3cdb93598a5ca5283461363f81c489a23b0672dd7d42cbb49c12635df251d153
    ],
    [
        0x0e59e6f332d7ed3720724b927a0ca81c4ad0447045a7c5aa2861ce16f219d5a9,
        0x1b064342d51a42753d7369467222697a172cc07b9d33fbf943b0a3fcff2036bd,
        0x30b82a998cbd8e8a2f363c55b2882e0b78fa9fb9171221b73eb310228a0e5f6c
    ],
    [
        0x23e4ab37183acba463df7a76e858a4aa8ad71ea715be0573e46f6d4298740107,
        0x2795d5c5fa4280225d33094e0beda75bacfe14640de044f2fca995e2b59914a1,
        0x3001ca401e89601cd765f26dd03f4c45a6687c3df16c8fe4c26d909dee8b53c0
    ],
    [
        0x0072e45cc676b08ef7bf86e89280827fe84b5bebae4e501de7fea6bdf3471380,
        0x13de705484874bb5e2abe4c518ce599eb64829e2d40e41bdd0c54ddeb26b86c0,
        0x0408a9fcf9d61abf315950f1211defe882bb18e5af1b05bb38915b432a9959a5
    ],
    [
        0x2780b9e75b55676ebb4e4a1400ccd2c4ae4d23b0b41be9a834070cbee26886a0,
        0x3a570d4d7c4e7ac3f80333ec85634ac9dc4d8fbefe24405a9405592098b4056f,
        0x0c13cca7cb1f9d2cf347c247fcf09294e2cc1507bebdcc6278d2b247899520b4
    ],
    [
        0x14f59baa03cd0ca4d2614a197c6b794b0b50bb2eb82df74d2e8c88f7707470e0,
        0x307defee925dfb436f546e1704c39c60a51d54ede66167f5be52476e0a16f3be,
        0x1960cd511a91e0607a07e7674b5a2621661106836adfe5e7380b67d80473dce3
    ],
    [
        0x2301ef9c63ea84c5ca2ad0fb56672500b8ee335d88284cbe15aaf1f7712589dd,
        0x029a5a47da79a488d10f4cd52be97f6bc86182d1b4246b585e68478c4d6027a9,
        0x32d7b16a7f11cc962360d17d890e55cbf97fe46b6a9254282cc4f962eaae2260
    ],
    [
        0x26703e48c03b81ca18e857a98d498cf7a5f2404cd7b35eb0c0cab915d5363d9f,
        0x048682a35b3265bc88ac8d25a24603f1f44388bd6b89221ef691123ae112b928,
        0x06b1390441fa7030d72cddc6cf06b50791d6e1715164775e3ab7defcb8d803e2
    ],
    [
        0x31aa0eeb868c626d1689426dce05fcd843b360f6386a86d7bcd795414a6e2e86,
        0x239464f75bf7b6af057abad3764c104b90efd8f41b2078b2ed77f5d576b99cc3,
        0x0a64d4c04fd426bda45e19ed813a54aba5cc47c59654b2a7b2cb487307c1cecf
    ],
    [
        0x21fbbdbb73670734576a4ad259860fb1777c7a921a062e9d1f7315322f658735,
        0x31b86f3cf01705d4d9371ca2eb95acf35b86d29463d31564674324003fc52146,
        0x2bfde53354377c9105ef1736d09056f613541d65157ee1ce7045f48aa4eb4f6f
    ],
    [
        0x1233ca936ec24671d558f36e65f8eca7f4d5239c11d0eafa5a13a58d20011e2f,
        0x27d452a43ac7dea2c437846d8e0b2b30878058d0234a576f6e70af0a7a924b3a,
        0x2699dba82184e413e816ea8da493e0fa6a30641a1c3d87b2a02576b94392f980
    ],
    [
        0x36c722f0efcc8803c3988baee42e4b10f18584664f8cab49608c6f7a61b56e55,
        0x02b3ff48861e339b08b0f2ec89ccaa3785c38899a7b5a8336e49ac170dbb7fcd,
        0x0b70d061d58d8a7f60162f4427bc657b6fc3ff4c49eb59ada8c5ae03ad98e405
    ],
    [
        0x3fc2a13f127f96a4f8753adeb9d7cee2ad3de8be46ed96932e06cc4af33b0a06,
        0x0c41a6e48dd23a511bd63434ac8c419f00cb3d621e171d80c12080ac117ee15f,
        0x2de8072a6bd86884ed4476537169084e72aaad7e4e75339d9685213e9692f5e1
    ],
    [
        0x03557a8f7b38a17f9d3496a3d9fe05ecb81cf735cc9c39c00ad01184567b027c,
        0x0b5f59552f498735ee976d34282f1a37060f43363d818e5445bcb5ac00826abc,
        0x0e2923a5fee7b878fedbb18570dc7300f5d646e57507e5482f2909e17e22b0df
    ],
    [
        0x1d785005a7a00592c787be97020a7fddcf1cb37c3b032af6f71eed73f15b3326,
        0x1ad772c273d9c6df0ba5fedcb8f25bd2a590b88a3b0602940acfbfb223f8f00d,
        0x027bd64785fcbd2aa78f3275c278234b810510eb61f0672dc1ce13d60f2f5031
    ],
    [
        0x20800f441b4a0526ce6f8ffea1031b6de224313469457b8e8337f5e07923a853,
        0x3d5ad61d7b65f9386eea2cd49f4312b436cdc8eed662ad37a33d7bed89a4408a,
        0x13338bc351fc46dd02c5f91be4dd8e3d1df96cc03ea4b26d3bbbae94cc195284
    ],
    [
        0x25e52be507c92760b87db1e2af3ea923646c49f9b46cbf19c5271c297852819e,
        0x1c492d64c157aaa471096d8b1b983c98a34c83a3485c6b2d5c380ab701b52ea9,
        0x0c5b801579992718f4e6c5e7a573f592d43487bc288df682a20c0b3da0da4ca3
    ],
    [
        0x1090b1b4d2bebe7a68695c0cd7cbf43d584e9e62a7f9554e7ea33c93e40833cf,
        0x33e38018a801387a68f5ce5cbed19cad1b218e35ecf2328ee383e1ec3baa8d69,
        0x1654af18772b2da5eef8d83d0e876bac5f4a02d28729e3aeb76b0b3d787ee953
    ],
    [
        0x1678be3cc9c6799344742de88c5ab0d5bb0893870367ec6cef7ce6a013265477,
        0x3780bd1e01f34c227ff9c6be546e928adaf1818355b13b4faf5d47893348f766,
        0x1e83d6315c9f125b0786018e7cb772675d11e69aa6c0b98ca12380320d7cc1de
    ],
    [
        0x354afd0a2f9d0b26160b41552f2931c8c486894d76e0c33b1799603e855ce731,
        0x00cd6d29f166eadc2d8affa62905c5a560b00dbe1faced078b997ee06be1bff3,
        0x1d6219352768e3aedbe0e3d7cdbc66efc60d01973f18305708d0641917082f2c
    ],
    [
        0x146336e25db5181de48d2370d7d1a142afe3ca1db8d4f529fa08dd9806387577,
        0x0005d8e085fd72ee997a21163e2e43df022e54b49c13d907a901d3ce84de0ad4,
        0x364e97c7a38932270dd5e61c8a4e86426f8ebc1d2296021a1c36f31341964484
    ],
    [
        0x01189910671bc16b561c6fff15346878fa97ec80ad307a52d7a00c03d2e0baaa,
        0x162a7c80f4d2d12e5229dfaa01231a454c0f7e001df490aa63fd8ac57a95ca8c,
        0x2a0d6c09576666bb2604e4afb09f8603caff31b4fda3212432e69efb22f40b96
    ],
    [
        0x0978e5c51e1e5649e16a4d603d5a808ef444d10d63a74e2cc0a0180f8cbfc0d2,
        0x1bdcee3aaca9cd25ebe19bbdce25101105087d903bdacfd103f4460ebc351b6e,
        0x1862cccb70b5b885e49479140b1944fd0c947321e0075e3ff61964bf3ade7670
    ],
    [
        0x1f3e91d863c16922bc26cc883a1987e139ee99c1cc6e5ddac3267da6e94adc50,
        0x1af47a48a6016a49ef5c08f8478f663afa661465c656ad990f85b4ac2c367406,
        0x3c8ee901956e3d3f009d57338c6935051c3698b0a2e3da100eabcd87e7d01b15
    ],
    [
        0x1660a8cde7fec55368d0b024f591b520e10ce2b7069f4dbd8b94772189673476,
        0x0f6d991929d5e4e71303936334dd11323963c2c1f5586e2f9d8d0f67fdaa79d5,
        0x02b9cea1921cd9f6cc625eaaab52b4dc4e7fda770712f3437a433091e1ce2d3a
    ],
    [
        0x14a323b99b900331214f7c6784acb565d8caf468976f04723797b2d8376043b3,
        0x190476b580cb9277ec01ea79642d5760718b7fbc7788af78347fef2c00f0953a,
        0x090a3a9d869d2eefa42463d30b442b6f9660902b60087651ff4e7e6fb268dfd7
    ],
    [
        0x3877a955863675670dbe8fd2270a6795e365001304f9a11ef983387ea0456203,
        0x2d894691240fe9535df39a2cc63ddc0a60118c53a218135239c0af0fe01f4a06,
        0x21b9c18292bdbc597ef71780201661895914e855eeb44aa11aca9eaf9bba9850
    ],
    [
        0x2fe76be7cff723e2505a05f2a6ae834c272e1cc6c36a296833f509a74ad9d39b,
        0x187aa448f391e3ca929981d7cfce253bd15bff840ddae8a50df9fa97277fa8b4,
        0x0b7083ad751707bf007ab3aa3617f422663ccf7b2ffe4b5ef0c66af5ffc73736
    ],
    [
        0x030ddbb470493f163bc4ca9902c52acb1975b962f6cb8e0b2f9b20f1fbd49791,
        0x3130fbaffb5aa82a950b0ab18d3546df8fb8ab9d60ea17b23a1c62ca8fbf2525,
        0x337f544707c430f04f74d74bac2ee45715ce2ead2fcd051e43a876180dc382e0
    ],
    [
        0x349979919015394fac9d91b0930dac757d8e471a9fb95fef26de98a8736d1d11,
        0x027cc4efe3fb35dd2305cd7a921ec5f13bf93da6fff31d95ccfcb61831d5c775,
        0x037f9f2365954c5b61b71a3698682ad267f1c6b7314764afc3fa2629635d27de
    ],
    [
        0x1f697cac4d07feb710f1cc6df8b4bcd760414abe362d01c977c5b024848371ae,
        0x267a750fe5d7cfbc26e6c851fbd572a63145c478063109d6786add244aa0ef29,
        0x0c91feab4a43193a678c9996d9a472c8af285fa82ce4fae5180e2b4d3e756f65
    ],
    [
        0x1745569a0a3e30142186c3038ea05e697e3b83af4a4ba3ba79c47c573ac410f7,
        0x29863d546e7e7c0deca5120778a56711fdff66c6f3b5ffe11e0388522696191f,
        0x1148d6ab2bd00192bf06bae49ef853f6a79a03df833994c62f225e6366bfe390
    ],
    [
        0x02e0e121b0f3dfefe18b1499060da366f745f45d350d41d4f4f6331a8b265d15,
        0x0d0aa46e76a6a278b89ef73a40a2b274690401736d44a653078ae6aa151054b7,
        0x13943675b04aa986eee545f3fa6d3d08392dde710f1f06db9a4d532c7b6e0958
    ],
    [
        0x2901ec61942d34aad97a11d63088f5d9c9f2b3257530dafe961fc818dcbb66b5,
        0x20204a2105d22e7ef431d54434a3e0cf22ffa2a2af9fa3e3fdf544b963d1fdc7,
        0x3a8a628295121d5c5c1e3e9e27a571c3a004abe8e01528c41211b9e2190d6852
    ],
]

MDS_matrix = [
    [
        0x0ab5e5b874a68de7b3d59fbdc8c9ead497d7a0ab23850b56323f2486d7e11b63,
        0x31916628e58a5abb293f0f0d886c7954240d4a7cbf7357368eca5596e996ab5e,
        0x07c045d5f5e9e5a6d803952bbb364fdfa0a3b71a5fb1573519d1cf25d8e8345d
    ],
    [
        0x233162630ebf9ed7f8e24f66822c2d9f3a0a464048bd770ad049cdc8d085167c,
        0x25cae2599892a8b0b36664548d60957d78f8365c85bbab07402270113e047a2e,
        0x22f5b5e1e6081c9774938717989a19579aad3d8262efd83ff84d806f685f747a
    ],
    [
        0x2e29dd59c64b1037f333aa91c383346421680eabc56bc15dfee7a9944f84dbe4,
        0x1d1aab4ec1cd678892d15e7dceef1665cbeaf48b3a0624c3c771effa43263664,
        0x3bf763086a18936451e0cbead65516b975872c39b59a31f615639415f6e85ef1
    ],
]

# Width
T = 3
# Full rounds
R_F = 8
# Partial rounds
R_P = 56
# Sponge rate
RATE = 2

# pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
Fp = IntegersModP(p)

MDS_MATRIX = numpy.array([[Fp(0)] * T] * T)
ROUND_CONSTANTS = []

for i in range(0, T):
    for j in range(0, T):
        MDS_MATRIX[i][j] = Fp(MDS_matrix[i][j])

for i in range(0, R_F + R_P):
    for j in range(0, T):
        ROUND_CONSTANTS.append(Fp(round_constants[i][j]))


def perm(inp):
    half_full_rounds = int(R_F / 2)
    state_words = numpy.array(inp)
    rcf = ROUND_CONSTANTS.copy()

    # First full rounds
    for _ in range(0, half_full_rounds):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        for i in range(0, T):
            state_words[i] = (state_words[i])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Middle partial rounds
    for _ in range(0, R_P):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        state_words[0] = (state_words[0])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Last full rounds
    for _ in range(0, half_full_rounds):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        for i in range(0, T):
            state_words[i] = (state_words[i])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    return state_words


def debug(n, s, m):
    if enable_debug:
        print(f"State {n} absorb:")
        pprint([hex(int(i)) for i in s])
        print(f"Mode {n} absorb:")
        pprint([hex(int(i)) if i is not None else None for i in m])


def poseidon_hash(messages):
    L = len(messages)
    k = int((L + RATE - 1) / RATE)
    padding = [Fp(0)] * (k * RATE - L)
    messages.extend(padding)

    # Sponge
    mode = [None] * RATE
    output = [None] * RATE
    state = [Fp(0)] * T

    # Capacity value is L ⋅ 2^64 + (o-1) where o is the output length
    initial_capacity_element = Fp(L << 64)
    state[RATE] = initial_capacity_element

    # This outermost loop absorbs the messages in the sponge.
    for n, value in enumerate(messages):
        debug(f"before {n+1}", state, mode)
        loop = False  # Use this to mark we should reiterate
        for i in range(0, len(mode)):
            if mode[i] is None:
                mode[i] = value
                loop = True
                break

        if loop:
            debug(f"after {n+1}", state, mode)
            continue

        # zip short-circuits when one iterator completes, so this will
        # only mutate the rate portion of the state.
        for i, _ in enumerate(zip(state, mode)):
            state[i] += mode[i]

        # Permutation of the current state
        state = perm(state)

        for i, _ in enumerate(zip(output, state)):
            output[i] = state[i]

        # Reinit sponge with the current message as the first value.
        mode = [None] * RATE
        mode[0] = value

        debug(f"after {n+1}", state, mode)

    debug("before final", state, mode)
    for i, _ in enumerate(zip(state, mode)):
        state[i] += mode[i]

    # Permutation of the final state
    state = perm(state)

    for i, _ in enumerate(zip(output, state)):
        output[i] = state[i]

    # Sponge now has the output, so the first element is our hash.
    mode = output
    debug("after final", state, mode)
    return output[0]


if __name__ == "__main__":
    enable_debug = False
    if enable_debug:
        from pprint import pprint

    #input_words = []
    #for i in range(0, T):
    #    input_words.append(Fp(i))
    #output_words = perm(input_words)
    #print([hex(int(i)) for i in output_words])

    words = []
    for i in range(0, 10):
        words.append(Fp(i))
        h = poseidon_hash(words.copy())
        print(hex(int(h)))
