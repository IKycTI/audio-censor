use clap::Subcommand;

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum ControlCommand {
    /// Показать текущие метрики демона
    Status,
    /// Приостановить обработку без остановки процесса
    Pause,
    /// Возобновить обработку после Pause
    Resume,
    /// Перечитать словарь запрещённых слов без перезапуска
    ReloadDict,
    /// Корректно остановить демон
    Shutdown,
}

impl ControlCommand {
    pub fn as_wire(&self) -> &'static str {
        match self {
            ControlCommand::Status => "status",
            ControlCommand::Pause => "pause",
            ControlCommand::Resume => "resume",
            ControlCommand::ReloadDict => "reload_dict",
            ControlCommand::Shutdown => "shutdown",
        }
    }

    pub fn parse(cmd: &str) -> Result<Self, String> {
        match cmd.trim() {
            "status" => Ok(ControlCommand::Status),
            "pause" => Ok(ControlCommand::Pause),
            "resume" => Ok(ControlCommand::Resume),
            "reload_dict" => Ok(ControlCommand::ReloadDict),
            "shutdown" => Ok(ControlCommand::Shutdown),
            other => Err(format!("неизвестная команда: {}", other)),
        }
    }
}
