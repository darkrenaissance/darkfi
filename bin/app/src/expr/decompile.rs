/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use super::{Op, SExprCode};

/// Renders an expr op tree back into a human-readable string. Used by the
/// netdebug backend so debug clients can see what an expr does. The output
/// is informational: it recompiles to an equivalent expr when restricted to
/// the compiler-expressible subset, but ops without source syntax render in
/// prefix form and u32 constants are tagged to keep their u32-ness visible.
pub fn decompile(code: &SExprCode) -> String {
    let stmts: Vec<String> = code.iter().map(render_op).collect();
    stmts.join("; ")
}

fn render_op(op: &Op) -> String {
    match op {
        Op::Null => "null".to_string(),
        Op::Add((lhs, rhs)) => format!("({} + {})", render_op(lhs), render_op(rhs)),
        Op::Sub((lhs, rhs)) => format!("({} - {})", render_op(lhs), render_op(rhs)),
        Op::Mul((lhs, rhs)) => format!("({} * {})", render_op(lhs), render_op(rhs)),
        Op::Div((lhs, rhs)) => format!("({} / {})", render_op(lhs), render_op(rhs)),
        Op::ConstBool(val) => format!("{val}"),
        Op::ConstUint32(val) => format!("u32({val})"),
        Op::ConstFloat32(val) => format!("{val}"),
        Op::ConstStr(val) => format!("{val:?}"),
        Op::LoadVar(var) => var.clone(),
        Op::StoreVar((var, val)) => format!("{var} = {}", render_op(val)),
        Op::Min((lhs, rhs)) => format!("min({}, {})", render_op(lhs), render_op(rhs)),
        Op::Max((lhs, rhs)) => format!("max({}, {})", render_op(lhs), render_op(rhs)),
        Op::IsEqual((lhs, rhs)) => format!("({} == {})", render_op(lhs), render_op(rhs)),
        Op::LessThan((lhs, rhs)) => format!("({} < {})", render_op(lhs), render_op(rhs)),
        Op::Float32ToUint32(val) => format!("as_u32({})", render_op(val)),
        Op::IfElse((cond, if_val, else_val)) => {
            format!(
                "if {} {{ {} }} else {{ {} }}",
                render_op(cond),
                decompile(if_val),
                decompile(else_val)
            )
        }
        Op::NativeFn(_) => "<native_fn>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{Compiler, NativeFnCallback, SExprVal},
        *,
    };

    #[test]
    fn render_null() {
        assert_eq!(decompile(&vec![Op::Null]), "null");
    }

    #[test]
    fn render_arith() {
        let code = vec![Op::Add((
            Box::new(Op::ConstUint32(5)),
            Box::new(Op::Div((
                Box::new(Op::LoadVar("sw".to_string())),
                Box::new(Op::ConstUint32(2)),
            ))),
        ))];
        assert_eq!(decompile(&code), "(u32(5) + (sw / u32(2)))");
    }

    #[test]
    fn render_consts() {
        assert_eq!(decompile(&vec![Op::ConstBool(true)]), "true");
        assert_eq!(decompile(&vec![Op::ConstBool(false)]), "false");
        assert_eq!(decompile(&vec![Op::ConstUint32(110)]), "u32(110)");
        assert_eq!(decompile(&vec![Op::ConstFloat32(2.5)]), "2.5");
        assert_eq!(decompile(&vec![Op::ConstFloat32(200.)]), "200");
        assert_eq!(decompile(&vec![Op::ConstStr("hello".to_string())]), "\"hello\"");
    }

    #[test]
    fn render_vars() {
        assert_eq!(decompile(&vec![Op::LoadVar("w".to_string())]), "w");
        let code = vec![Op::StoreVar((
            "r".to_string(),
            Box::new(Op::Div((Box::new(Op::ConstFloat32(10.)), Box::new(Op::ConstFloat32(4.))))),
        ))];
        assert_eq!(decompile(&code), "r = (10 / 4)");
    }

    #[test]
    fn render_ops_without_source_syntax() {
        let lhs = Box::new(Op::LoadVar("a".to_string()));
        let rhs = Box::new(Op::LoadVar("b".to_string()));
        assert_eq!(decompile(&vec![Op::Min((lhs.clone(), rhs.clone()))]), "min(a, b)");
        assert_eq!(decompile(&vec![Op::Max((lhs.clone(), rhs.clone()))]), "max(a, b)");
        assert_eq!(decompile(&vec![Op::IsEqual((lhs.clone(), rhs.clone()))]), "(a == b)");
        assert_eq!(decompile(&vec![Op::LessThan((lhs, rhs))]), "(a < b)");
        assert_eq!(
            decompile(&vec![Op::Float32ToUint32(Box::new(Op::ConstFloat32(1.6)))]),
            "as_u32(1.6)"
        );
    }

    #[test]
    fn render_if_else() {
        let code = vec![Op::IfElse((
            Box::new(Op::LessThan((
                Box::new(Op::LoadVar("h".to_string())),
                Box::new(Op::ConstFloat32(4.)),
            ))),
            vec![Op::Sub((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(1.))))],
            vec![Op::Add((
                Box::new(Op::Mul((
                    Box::new(Op::ConstFloat32(2.)),
                    Box::new(Op::LoadVar("h".to_string())),
                ))),
                Box::new(Op::ConstFloat32(5.)),
            ))],
        ))];
        assert_eq!(decompile(&code), "if (h < 4) { (h - 1) } else { ((2 * h) + 5) }");
    }

    #[test]
    fn render_native_fn() {
        let code = vec![Op::NativeFn(NativeFnCallback(|_| Ok(SExprVal::Null)))];
        assert_eq!(decompile(&code), "<native_fn>");
    }

    #[test]
    fn render_stmts() {
        let code = vec![
            Op::StoreVar(("r".to_string(), Box::new(Op::ConstFloat32(1.)))),
            Op::Add((Box::new(Op::LoadVar("r".to_string())), Box::new(Op::ConstFloat32(1.)))),
        ];
        assert_eq!(decompile(&code), "r = 1; (r + 1)");
    }

    #[test]
    fn decompile_compiled() {
        let cc = Compiler::new();
        let code = cc.compile("h/2 - 200").unwrap();
        assert_eq!(decompile(&code), "((h / 2) - 200)");
    }

    #[test]
    fn roundtrip() {
        let cc = Compiler::new();
        let fixtures = [
            "h/2 - 200",
            "(x + h/2 + (y + 7)/5) - 200",
            "h - 1",
            "r = 10 / 4",
            "r = 10 / 4;\n\ns = if h < 4 {\n h - 1\n} else {\n 2 * h + 5\n};\n\nr + 1",
        ];
        for src in fixtures {
            let once = cc.compile(src).unwrap();
            let src2 = decompile(&once);
            let twice = cc.compile(&src2).unwrap();
            assert_eq!(once, twice, "fixture: {src} rendered as: {src2}");
        }
    }
}
