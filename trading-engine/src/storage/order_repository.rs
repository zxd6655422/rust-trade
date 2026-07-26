// storage/order_repository.rs
// 订单仓储层

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::exchange::types::{OrderInfo, OrderSide, OrderStatus, OrderType, TimeInForce};

/// 订单来源
#[derive(Debug, Clone, PartialEq)]
pub enum OrderSource {
    /// 程序自动下单
    Auto,
    /// 手动下单（交易所 APP/网页/API 手动调用）
    Manual,
    /// 未知
    Unknown,
}

impl OrderSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSource::Auto => "auto",
            OrderSource::Manual => "manual",
            OrderSource::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "auto" => OrderSource::Auto,
            "manual" => OrderSource::Manual,
            _ => OrderSource::Unknown,
        }
    }
}

/// 订单记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrderRecord {
    pub id: Uuid,
    pub order_id: String,
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub status: String,
    pub filled_quantity: Decimal,
    pub avg_price: Option<Decimal>,
    pub commission: Option<Decimal>,
    pub commission_asset: Option<String>,
    pub client_order_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // 新增字段
    pub market_type: String,
    pub uid: Option<String>,
    pub position_side: Option<String>,
    pub source: String,
    pub signal_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub time_in_force: Option<String>,
    pub stop_price: Option<Decimal>,
}

/// 订单仓储
pub struct OrderRepository {
    pool: PgPool,
}

impl OrderRepository {
    /// 创建新的订单仓储
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 创建订单
    pub async fn create_order(
        &self,
        order_id: &str,
        exchange: &str,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: Decimal,
        price: Option<Decimal>,
        client_order_id: Option<String>,
    ) -> Result<OrderRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, OrderRecord>(
            r#"
            INSERT INTO trading_orders (order_id, exchange, symbol, side, order_type, quantity, price, status, client_order_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'NEW', $8)
            RETURNING *
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .bind(symbol)
        .bind(side)
        .bind(order_type)
        .bind(quantity)
        .bind(price)
        .bind(client_order_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 创建订单（带完整信息，用于自动交易）
    #[allow(clippy::too_many_arguments)]
    pub async fn create_order_full(
        &self,
        order_id: &str,
        exchange: &str,
        market_type: &str,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: Decimal,
        price: Option<Decimal>,
        client_order_id: Option<String>,
        source: OrderSource,
        uid: Option<String>,
        position_side: Option<String>,
        signal_id: Option<Uuid>,
        strategy_id: Option<String>,
        time_in_force: Option<String>,
        stop_price: Option<Decimal>,
    ) -> Result<OrderRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, OrderRecord>(
            r#"
            INSERT INTO trading_orders (
                order_id, exchange, market_type, symbol, side, order_type,
                quantity, price, status, client_order_id,
                source, uid, position_side, signal_id, strategy_id,
                time_in_force, stop_price
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'NEW', $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING *
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .bind(market_type)
        .bind(symbol)
        .bind(side)
        .bind(order_type)
        .bind(quantity)
        .bind(price)
        .bind(client_order_id)
        .bind(source.as_str())
        .bind(uid)
        .bind(position_side)
        .bind(signal_id)
        .bind(strategy_id)
        .bind(time_in_force)
        .bind(stop_price)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 更新订单状态
    pub async fn update_order_status(
        &self,
        order_id: &str,
        exchange: &str,
        status: &str,
        filled_quantity: Decimal,
        avg_price: Option<Decimal>,
        commission: Option<Decimal>,
        commission_asset: Option<String>,
    ) -> Result<OrderRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, OrderRecord>(
            r#"
            UPDATE trading_orders
            SET status = $3,
                filled_quantity = $4,
                avg_price = $5,
                commission = $6,
                commission_asset = $7,
                updated_at = NOW()
            WHERE order_id = $1 AND exchange = $2
            RETURNING *
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .bind(status)
        .bind(filled_quantity)
        .bind(avg_price)
        .bind(commission)
        .bind(commission_asset)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 获取订单
    pub async fn get_order(
        &self,
        order_id: &str,
        exchange: &str,
    ) -> Result<Option<OrderRecord>, sqlx::Error> {
        let record = sqlx::query_as::<_, OrderRecord>(
            r#"
            SELECT * FROM trading_orders
            WHERE order_id = $1 AND exchange = $2
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    /// 获取活动订单
    pub async fn get_active_orders(
        &self,
        exchange: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<OrderRecord>, sqlx::Error> {
        let records = if let Some(s) = symbol {
            sqlx::query_as::<_, OrderRecord>(
                r#"
                SELECT * FROM trading_orders
                WHERE exchange = $1 AND symbol = $2 AND status IN ('NEW', 'PARTIALLY_FILLED')
                ORDER BY created_at DESC
                "#,
            )
            .bind(exchange)
            .bind(s)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, OrderRecord>(
                r#"
                SELECT * FROM trading_orders
                WHERE exchange = $1 AND status IN ('NEW', 'PARTIALLY_FILLED')
                ORDER BY created_at DESC
                "#,
            )
            .bind(exchange)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(records)
    }

    /// 获取订单历史
    pub async fn get_order_history(
        &self,
        exchange: &str,
        symbol: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OrderRecord>, sqlx::Error> {
        let records = if let Some(s) = symbol {
            sqlx::query_as::<_, OrderRecord>(
                r#"
                SELECT * FROM trading_orders
                WHERE exchange = $1 AND symbol = $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
            )
            .bind(exchange)
            .bind(s)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, OrderRecord>(
                r#"
                SELECT * FROM trading_orders
                WHERE exchange = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
            )
            .bind(exchange)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(records)
    }

    /// 删除订单
    pub async fn delete_order(&self, order_id: &str, exchange: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM trading_orders
            WHERE order_id = $1 AND exchange = $2
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 转换为 OrderInfo
    pub fn to_order_info(record: &OrderRecord) -> OrderInfo {
        OrderInfo {
            order_id: record.order_id.clone(),
            client_order_id: record.client_order_id.clone(),
            symbol: record.symbol.clone(),
            side: match record.side.as_str() {
                "BUY" => OrderSide::Buy,
                _ => OrderSide::Sell,
            },
            order_type: match record.order_type.as_str() {
                "MARKET" => OrderType::Market,
                "LIMIT" => OrderType::Limit,
                "STOP_LOSS" => OrderType::StopLoss,
                "TAKE_PROFIT" => OrderType::TakeProfit,
                _ => OrderType::Market,
            },
            status: match record.status.as_str() {
                "NEW" => OrderStatus::New,
                "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
                "FILLED" => OrderStatus::Filled,
                "CANCELED" => OrderStatus::Canceled,
                "PENDING_CANCEL" => OrderStatus::PendingCancel,
                "REJECTED" => OrderStatus::Rejected,
                "EXPIRED" => OrderStatus::Expired,
                _ => OrderStatus::New,
            },
            quantity: record.quantity,
            filled_quantity: record.filled_quantity,
            remaining_quantity: record.quantity - record.filled_quantity,
            price: record.price,
            stop_price: record.stop_price,
            time_in_force: TimeInForce::Gtc,
            created_at: record.created_at,
            updated_at: record.updated_at,
            signal_stop_loss: None,  // 从DB加载的历史订单没有策略止损价
        }
    }

    /// 检查订单是否已存在
    pub async fn order_exists(
        &self,
        order_id: &str,
        exchange: &str,
    ) -> Result<bool, sqlx::Error> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1 FROM trading_orders WHERE order_id = $1 AND exchange = $2)
            "#,
        )
        .bind(order_id)
        .bind(exchange)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    /// 批量检查订单是否存在（用于识别手动订单）
    pub async fn filter_existing_orders(
        &self,
        order_ids: &[String],
        exchange: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let existing: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT order_id FROM trading_orders
            WHERE exchange = $1 AND order_id = ANY($2)
            "#,
        )
        .bind(exchange)
        .bind(order_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(existing.into_iter().map(|(id,)| id).collect())
    }
}
