use crate::wallet::CashierDb;
use crate::{Result};
use async_std::sync::Arc;


pub struct CashierAdapter {
    pub wallet: Arc<CashierDb>,
}

impl CashierAdapter {
    pub fn new(wallet: Arc<CashierDb>) -> Result<Self> {
        Ok(Self { wallet })
    }

    pub fn handle_input(
        self: Arc<Self>,
    ) -> Result<jsonrpc_core::IoHandler> {
        let mut io = jsonrpc_core::IoHandler::new();
        io.add_sync_method("cashier_hello", |_| {
            Ok(jsonrpc_core::Value::String("hello world!".into()))
        });
        Ok(io)
        //    let self1 = self.clone();
        //    io.add_method("get_key", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            let pub_key = self2.get_key()?;
        //            Ok(jsonrpc_core::Value::String(pub_key))
        //        }
        //    });

        //    let self1 = self.clone();
        //    io.add_method("get_cash_public", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            let cash_key = self2.get_cash_public()?;
        //            Ok(jsonrpc_core::Value::String(cash_key))
        //        }
        //    });

        //    let self1 = self.clone();
        //    io.add_method("get_info", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            self2.get_info();
        //            Ok(jsonrpc_core::Value::Null)
        //        }
        //    });

        //    let self1 = self.clone();
        //    io.add_method("stop", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            self2.stop();
        //            Ok(jsonrpc_core::Value::Null)
        //        }
        //    });
        //    let self1 = self.clone();

        //    io.add_method("create_wallet", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            self2.init_db()?;
        //            Ok(jsonrpc_core::Value::String(
        //                "wallet creation successful".into(),
        //            ))
        //        }
        //    });

        //    let self1 = self.clone();
        //    io.add_method("key_gen", move |_| {
        //        let self2 = self1.clone();
        //        async move {
        //            self2.key_gen()?;
        //            Ok(jsonrpc_core::Value::String(
        //                "key generation successful".into(),
        //            ))
        //        }
        //    });
    }
}
