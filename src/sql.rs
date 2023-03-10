use std::collections::HashMap;

use convert_case::{Case, Casing};

use crate::UrlQuery;

pub enum Database {
    Postgres,
    MySQL,
}

/// Generates an SQL query
///
/// # Examples
///
/// ```
/// use std::collections::{HashMap, HashSet};
/// use query::{UrlQuery, sql::QueryBuilder};
///
/// let query = "userId=123&userName=bob";
///
/// let parsed = UrlQuery::new(query, ["userId", "userName"]).unwrap();
///
/// let (sql, args) = QueryBuilder::from_str("SELECT id, status FROM orders", parsed).build();
///
/// assert_eq!(sql, "SELECT id, status FROM orders WHERE userId = $1 AND userName = $2");
/// assert_eq!(args.len(), 2);
/// ```
pub struct QueryBuilder<'a> {
    url_query: UrlQuery,
    database: Database,
    map_columns: HashMap<&'a str, &'a str>,
    shift_bind: usize,
    convert_case: Option<Case>,
    sql: String,
}

impl<'a> QueryBuilder<'a> {
    /// Returns a QueryBuilder.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use query::sql::QueryBuilder;
    ///
    /// let (sql, args) = QueryBuilder::new("users", vec!["id", "first_name"], url_query).build();
    /// ```
    pub fn new(table: &str, columns: Vec<&str>, url_query: UrlQuery) -> Self {
        let sql = gen_sql_select(table, columns);

        Self {
            url_query,
            database: Database::Postgres,
            map_columns: HashMap::default(),
            shift_bind: 0,
            convert_case: None,
            sql,
        }
    }

    /// Returns a QueryBuilder.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use query::sql::QueryBuilder;
    ///
    /// let (sql, args) = QueryBuilder::from_str("SELECT * FROM users", url_query).build();
    /// ```
    pub fn from_str(sql: &str, url_query: UrlQuery) -> Self {
        Self {
            url_query,
            database: Database::Postgres,
            map_columns: HashMap::default(),
            shift_bind: 0,
            convert_case: None,
            sql: sql.into(),
        }
    }

    /// Set the database
    pub fn set_database(mut self, database: Database) -> Self {
        self.database = database;

        self
    }

    /// Append anything to the SQL.
    pub fn append(mut self, sql: &str) -> Self {
        self.sql.push_str(" ");
        self.sql.push_str(sql);

        self
    }

    /// Provide a HashMap containing table and column to map ambiguous columns.
    pub fn map_columns(mut self, map_columns: HashMap<&'a str, &'a str>) -> Self {
        self.map_columns = map_columns;

        self
    }

    /// Shifts the number of the bind parameter for postgres. For example, if you call this
    /// method with a value of 1, the first arg you'll need to bind to the SQL will be $2.
    pub fn shift_bind(mut self, x: usize) -> Self {
        self.shift_bind = x;

        self
    }

    pub fn convert_case(mut self, case: Case) -> Self {
        self.convert_case = Some(case);

        self
    }

    /// Append the WHERE clause to the SQL. Does nothing if there are no queries/filters in the url query.
    pub fn append_where(&mut self) -> Vec<(String, String)> {
        let mut args: Vec<(String, String)> = Vec::new();

        // Filters:
        let mut filterv = Vec::new();
        for filter in self.url_query.filters.iter() {
            let table = self.map_columns.get(filter.field.as_str());
            filterv.push(filter.to_sql_map_table(
                args.len() + self.shift_bind + 1,
                table,
                self.convert_case,
                &self.database,
            ));
            args.push((filter.field.to_owned(), filter.value.to_owned()));
        }
        let filter = filterv.join(" AND ");

        // WHERE clause
        if filterv.len() > 0 {
            self.sql.push_str(" WHERE ");
            self.sql.push_str(&filter);
        }

        args
    }

    /// Append a GROUP BY to the SQL. Does nothing if there is no group in the url query.
    pub fn append_group(&mut self) {
        if self.url_query.group.is_none() {
            return;
        };

        let group = self.url_query.group.as_ref().unwrap();
        self.sql.push_str(" GROUP BY ");
        if let Some(table) = self.map_columns.get(group.as_str()) {
            self.sql.push_str(table);
            self.sql.push_str(".");
        }

        match self.convert_case {
            Some(c) => self.sql.push_str(&group.to_case(c)),
            None => self.sql.push_str(&group),
        }
    }

    /// Append an ORDER BY to the SQL. Does nothing if there is no sort in the url query.
    pub fn append_sort(&mut self) {
        if self.url_query.sort.is_none() {
            return;
        }

        let sort = self.url_query.sort.as_ref().unwrap();
        let table = self.map_columns.get(sort.field.as_str());
        self.sql.push_str(" ORDER BY ");
        self.sql
            .push_str(&sort.to_sql_map_table(table, self.convert_case));
    }

    /// Returns SQL statement along with a list of columns and args to bind.
    pub fn build(mut self) -> (String, Vec<(String, String)>) {
        // returns bind args
        let args = self.append_where();

        self.append_group();

        self.append_sort();

        // Limit & offset:
        if let Ok(limit) = self.url_query.check_limit() {
            append_limit(&mut self.sql, limit);

            if let Ok(offset) = self.url_query.check_offset() {
                append_offset(&mut self.sql, offset);
            }
        }

        (self.sql, args)
    }
}

fn gen_sql_select(table: &str, columns: Vec<&str>) -> String {
    let mut sql = String::from("SELECT ");
    let columns = columns.join(", ");
    sql.push_str(&columns);
    sql.push_str(" FROM ");
    sql.push_str(table);
    sql
}

fn append_limit(sql: &mut String, limit: &str) {
    sql.push_str(" LIMIT ");
    sql.push_str(limit);
}

fn append_offset(sql: &mut String, offset: &str) {
    sql.push_str(" OFFSET ");
    sql.push_str(offset);
}

/// Bind args to an sqlx query with the required types.
///
/// ```ignore
/// pub async fn get_orders(
///     pool: &PgPool,
///     query: UrlQuery,
/// ) -> Result<Vec<Order>, Either<sqlx::Error, ParseError>> {
///     let (sql, args) = QueryBuilder::from_str(
///         "SELECT * FROM orders",
///         query,
///     )
///     .build();
///
///     let mut query = sqlx::query_as(&sql);
///
///     bind!(
///         args => query,
///         error: Either::Right(ParseError),
///         "id" => Uuid,
///         "userId" => i64
///     );
///
///     Ok(query.fetch_all(pool).await.map_err(|e| Either::Left(e))?)
/// }
/// ```
#[macro_export]
macro_rules! sqlx_bind {
    ( $args:ident => $query:ident, error: $error:expr, $( $x:expr => $t:ty ),* ) => {
        {
            for (column, arg) in $args {
                match column.as_str() {
                    $(
                        $x => {
                            let parsed: $t = arg.parse().map_err(|_| {
                                $error
                            })?;
                            $query = $query.bind(parsed);
                        }
                    )*
                    _ => {}
                }
            }
        }
    };
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use convert_case::Case;

    use crate::{sql::Database, UrlQuery};

    use super::QueryBuilder;

    #[test]
    fn test_query_builder_from_str() {
        let query =
            "userId=123&userName=bob&filter[]=orderId-eq-1&filter[]=price-ge-200&sort=price-desc&limit=10&offset=0";

        let parsed = UrlQuery::new(query, ["userId", "userName", "orderId", "price"]).unwrap();

        let (sql, args) = QueryBuilder::from_str("SELECT * FROM orders", parsed)
            .convert_case(Case::Snake)
            .build();

        let expected = "SELECT * FROM orders \
        WHERE user_id = $1 AND user_name = $2 \
        AND order_id = $3 AND price >= $4 \
        ORDER BY price DESC \
        LIMIT 10 \
        OFFSET 0";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_query_builder_new() {
        let query =
            "userId=123&userName=bob&filter[]=orderId-eq-1&filter[]=price-ge-200&sort=price-desc&limit=10&offset=0";

        let parsed = UrlQuery::new(query, ["userId", "userName", "orderId", "price"]).unwrap();

        let (sql, args) = QueryBuilder::new("orders", vec!["id", "status"], parsed)
            .convert_case(Case::Snake)
            .build();

        let expected = "SELECT id, status FROM orders \
        WHERE user_id = $1 AND user_name = $2 \
        AND order_id = $3 AND price >= $4 \
        ORDER BY price DESC \
        LIMIT 10 \
        OFFSET 0";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_query_builder_new_append_joins() {
        let query =
            "userId=123&userName=bob&filter[]=orderId-eq-1&filter[]=price-ge-200&sort=price-desc&limit=10&offset=0";

        let parsed = UrlQuery::new(query, ["userId", "userName", "orderId", "price"]).unwrap();

        let (sql, args) = QueryBuilder::new("orders", vec!["id", "status"], parsed)
            .append("JOIN users ON users.id = order.user_id")
            .append("JOIN inventory ON inventory.id = order.inventory_id")
            .convert_case(Case::Snake)
            .build();

        let expected = "SELECT id, status FROM orders \
        JOIN users ON users.id = order.user_id \
        JOIN inventory ON inventory.id = order.inventory_id \
        WHERE user_id = $1 AND user_name = $2 \
        AND order_id = $3 AND price >= $4 \
        ORDER BY price DESC \
        LIMIT 10 \
        OFFSET 0";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_query_builder_new_map_columns() {
        let query = "id=1&group=id&sort=createdAt-desc";

        let parsed = UrlQuery::new(query, ["id", "createdAt"]).unwrap();

        let (sql, args) = QueryBuilder::from_str(
            "SELECT orders.id, user_id, status, address_id, orders.created_at FROM orders",
            parsed,
        )
        .append("JOIN order_items ON orders.id = order_items.order_id")
        .append("JOIN inventory ON order_items.inventory_id = inventory.id")
        .map_columns(HashMap::from([("id", "orders"), ("createdAt", "orders")]))
        .convert_case(Case::Snake)
        .build();

        let expected =
            "SELECT orders.id, user_id, status, address_id, orders.created_at FROM orders \
             JOIN order_items ON orders.id = order_items.order_id \
             JOIN inventory ON order_items.inventory_id = inventory.id \
             WHERE orders.id = $1 GROUP BY orders.id ORDER BY orders.created_at DESC";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_append_where() {
        let query = "filter[]=userId-eq-1&filter[]=id-eq-2";

        let parsed = UrlQuery::new(query, ["userId", "id"]).unwrap();

        let mut builder = QueryBuilder::from_str("", parsed);

        let mut args = builder.append_where().into_iter();

        let user_id = args.next().unwrap().1;
        assert_eq!(user_id, "1");

        let id = args.next().unwrap().1;
        assert_eq!(id, "2");
    }

    #[test]
    fn test_shift_bind() {
        let query = "filter[]=userId-eq-1&filter[]=id-eq-2";

        let parsed = UrlQuery::new(query, ["userId", "id"]).unwrap();

        let (sql, args) = QueryBuilder::from_str(
            "SELECT id, (SELECT postcode FROM address WHERE id = $1) FROM orders",
            parsed,
        )
        .shift_bind(1)
        .convert_case(Case::Snake)
        .build();

        let expected = "SELECT id, (SELECT postcode FROM address WHERE id = $1) FROM orders WHERE user_id = $2 AND id = $3";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_query_builder_set_database_mysql() {
        let query =
            "userId=123&userName=bob&filter[]=orderId-eq-1&filter[]=price-ge-200&sort=price-desc&limit=10&offset=0";

        let parsed = UrlQuery::new(query, ["userId", "userName", "orderId", "price"]).unwrap();

        let (sql, args) = QueryBuilder::new("orders", vec!["id", "status"], parsed)
            .convert_case(Case::Snake)
            .set_database(Database::MySQL)
            .build();

        let expected = "SELECT id, status FROM orders \
        WHERE user_id = ? AND user_name = ? \
        AND order_id = ? AND price >= ? \
        ORDER BY price DESC \
        LIMIT 10 \
        OFFSET 0";

        assert_eq!(sql, expected);
        assert_eq!(args.len(), 4);
    }
}
