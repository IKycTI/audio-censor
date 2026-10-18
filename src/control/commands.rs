use clap::Subcommand;

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum ControlCommand {
    /// Показать текущие метрики демона
    Status,
    /// Перечитать словарь запрещённых слов без перезапуска
    ReloadDict,
    /// Выгрузить текущие метрики в файл сеанса (metrics.dir)
    SaveMetrics,
    /// Корректно остановить демон
    Shutdown,
}

impl ControlCommand {
    pub fn as_wire(&self) -> &'static str {
        match self {
            ControlCommand::Status => "status",
            ControlCommand::ReloadDict => "reload_dict",
            ControlCommand::SaveMetrics => "save_metrics",
            ControlCommand::Shutdown => "shutdown",
        }
    }

    pub fn parse(cmd: &str) -> Result<Self, String> {
        match cmd.trim() {
            "status" => Ok(ControlCommand::Status),
            "reload_dict" => Ok(ControlCommand::ReloadDict),
            "save_metrics" => Ok(ControlCommand::SaveMetrics),
            "shutdown" => Ok(ControlCommand::Shutdown),
            other => Err(format!("неизвестная команда: {}", other)),
        }
    }
}
