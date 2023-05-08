//! Mocked DEAP VMs for testing

use mpc_core::Block;
use mpc_ot::mock::{mock_ot_pair, MockOTReceiver, MockOTSender};
use utils_aio::mux::{mock::MockMuxChannelFactory, MuxChannelControl};

use crate::config::Role;

use super::vm::DEAPVm;

/// Create a pair of mocked DEAP VMs
pub async fn create_mock_deap_vm(
    id: &str,
) -> (
    DEAPVm<MockOTSender<Block>, MockOTReceiver<Block>>,
    DEAPVm<MockOTSender<Block>, MockOTReceiver<Block>>,
) {
    let mut mux_factory = MockMuxChannelFactory::new();
    let (leader_ot_send, follower_ot_recv) = mock_ot_pair();
    let (follower_ot_send, leader_ot_recv) = mock_ot_pair();

    let leader_channel = mux_factory.get_channel(id.to_string()).await.unwrap();
    let follower_channel = mux_factory.get_channel(id.to_string()).await.unwrap();

    let leader = DEAPVm::new(
        id,
        Role::Leader,
        [42u8; 32],
        leader_channel,
        Box::new(mux_factory.clone()),
        leader_ot_send,
        leader_ot_recv,
    );

    let follower = DEAPVm::new(
        id,
        Role::Follower,
        [69u8; 32],
        follower_channel,
        Box::new(mux_factory),
        follower_ot_send,
        follower_ot_recv,
    );

    (leader, follower)
}