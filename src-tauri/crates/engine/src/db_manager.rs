use duckdb::Connection;

use crate::EngineResult;

pub struct DbManager {
    conn: Connection,
}

impl DbManager {
    pub fn open_file(path: &str) -> EngineResult<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> EngineResult<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
