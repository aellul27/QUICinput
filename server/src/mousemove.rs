use shared::MouseMove;

#[cfg(target_os = "linux")]
use uinput::event::relative;

#[cfg(target_os = "linux")]
pub fn create_virtual_mouse() -> Result<uinput::Device, uinput::Error> {
    uinput::default()?
        .name("my-virtual-mouse")?
        .event(relative::Position::X)?
        .event(relative::Position::Y)?
        .create()
}

#[cfg(target_os = "linux")]
pub fn do_mouse_move(device: &mut uinput::Device, mousemove: MouseMove) -> Result<(), uinput::Error> {
    device.position(&relative::Position::X, mousemove.dx as i32)?;
    device.position(&relative::Position::Y, mousemove.dy as i32)?;
    device.synchronize()?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
use crate::simulator::EventSimulator;
#[cfg(not(target_os = "linux"))]
use mouse_position::mouse_position::Mouse;
#[cfg(not(target_os = "linux"))]
use rdev::EventType;

#[cfg(not(target_os = "linux"))]
pub fn do_mouse_move(simulator: &EventSimulator, mousemove: MouseMove) {
    match Mouse::get_mouse_position() {
        Mouse::Position { x, y } => {
            let event = EventType::MouseMove {
                x: x as f64 + mousemove.dx,
                y: y as f64 + mousemove.dy,
            };
            simulator.enqueue(event);
        }
        Mouse::Error => eprintln!("[server] failed to read mouse position"),
    }
}