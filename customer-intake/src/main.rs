#[tokio::main]
async fn main() -> anyhow::Result<()> {
    customer_intake::run().await
}
