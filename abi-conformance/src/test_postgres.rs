use crate::{
    postgres::{self, PgError},
    rdbms_types::{Column, DbDataType, DbValue, ParameterValue, RowSet},
    Context,
};
use anyhow::{ensure, Result};
use async_trait::async_trait;
use serde::Serialize;
use std::{collections::HashMap, iter};
use wasmtime::{component::InstancePre, Store};

/// Report of which PostgreSQL functions a module successfully used, if any
#[derive(Serialize, PartialEq, Eq, Debug)]
pub struct PostgresReport {
    /// Result of the PostgreSQL statement execution test
    ///
    /// The guest module should expect a call according to [`crate::InvocationStyle`] with \["postgres-execute",
    /// "127.0.0.1", "INSERT INTO foo (x) VALUES ($1)", "int8:42"\] as arguments.  The module should call the
    /// host-implemented `postgres::execute` function with the arguments \["127.0.0.1", "INSERT INTO foo (x)
    /// VALUES ($1)", `\[int8(42)\]`\] and expect `ok(1)` as the result.  The host will assert that said function
    /// is called exactly once with the specified arguments.
    pub execute: Result<(), String>,

    /// Result of the PostgreSQL query execution test
    ///
    /// The guest module should expect a call according to [`crate::InvocationStyle`] with \["postgres-query",
    /// "127.0.0.1", "SELECT x FROM foo"\] as arguments.  The module should call the host-implemented
    /// `postgres::execute` function with the arguments \["127.0.0.1", "SELECT x FROM foo"\] and expect `ok({
    /// columns: \[ { name: "x", data_type: int8 } \], rows: \[ \[ int8(42) \] \]})` as the result.  The host will
    /// assert that said function is called exactly once with the specified arguments.
    pub query: Result<(), String>,
}

#[derive(Default)]
pub(crate) struct Postgres {
    execute_map: HashMap<(String, String, String), u64>,
    query_map: HashMap<(String, String, String), RowSet>,
}

#[async_trait]
impl postgres::Host for Postgres {
    async fn execute(
        &mut self,
        address: String,
        statement: String,
        params: Vec<ParameterValue>,
    ) -> Result<Result<u64, PgError>> {
        Ok(self
            .execute_map
            .remove(&(
                address.to_owned(),
                statement.to_owned(),
                format!("{params:?}"),
            ))
            .ok_or_else(|| {
                PgError::OtherError(format!(
                    "expected {:?}, got {:?}",
                    self.execute_map.keys(),
                    iter::once(&(
                        address.to_owned(),
                        statement.to_owned(),
                        format!("{params:?}")
                    ))
                ))
            }))
    }

    async fn query(
        &mut self,
        address: String,
        statement: String,
        params: Vec<ParameterValue>,
    ) -> Result<Result<RowSet, PgError>> {
        Ok(self
            .query_map
            .remove(&(
                address.to_owned(),
                statement.to_owned(),
                format!("{params:?}"),
            ))
            .ok_or_else(|| {
                PgError::OtherError(format!(
                    "expected {:?}, got {:?}",
                    self.query_map.keys(),
                    iter::once(&(
                        address.to_owned(),
                        statement.to_owned(),
                        format!("{params:?}")
                    ))
                ))
            }))
    }
}

pub(crate) async fn test(
    store: &mut Store<Context>,
    pre: &InstancePre<Context>,
) -> Result<PostgresReport> {
    Ok(PostgresReport {
        execute: test_execute(store, pre).await,
        query: test_query(store, pre).await,
    })
}

async fn test_execute(
    store: &mut Store<Context>,
    pre: &InstancePre<Context>,
) -> Result<(), String> {
    store.data_mut().postgres.execute_map.insert(
        (
            "127.0.0.1".into(),
            "INSERT INTO foo (x) VALUES ($1)".into(),
            format!("{:?}", vec![ParameterValue::Int8(42)]),
        ),
        1,
    );

    crate::run_command(
        store,
        pre,
        &[
            "postgres-execute",
            "127.0.0.1",
            "INSERT INTO foo (x) VALUES ($1)",
            "int8:42",
        ],
        |store| {
            ensure!(
                store.data().postgres.execute_map.is_empty(),
                "expected module to call `postgres::execute` exactly once"
            );

            Ok(())
        },
    )
    .await
}

async fn test_query(store: &mut Store<Context>, pre: &InstancePre<Context>) -> Result<(), String> {
    let row_set = RowSet {
        columns: vec![Column {
            name: "x".into(),
            data_type: DbDataType::Int8,
        }],
        rows: vec![vec![DbValue::Int8(42)]],
    };

    store.data_mut().postgres.query_map.insert(
        (
            "127.0.0.1".into(),
            "SELECT x FROM foo".into(),
            format!("{:?}", Vec::<()>::new()),
        ),
        row_set,
    );

    crate::run_command(
        store,
        pre,
        &["postgres-query", "127.0.0.1", "SELECT x FROM foo"],
        |store| {
            ensure!(
                store.data().postgres.query_map.is_empty(),
                "expected module to call `postgres::query` exactly once"
            );

            Ok(())
        },
    )
    .await
}
