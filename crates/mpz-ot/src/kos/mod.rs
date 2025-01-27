//! An implementation of the [`KOS15`](https://eprint.iacr.org/2015/546.pdf) oblivious transfer extension protocol.

mod error;
mod receiver;
mod sender;
mod shared_receiver;
mod shared_sender;

pub use error::{ReceiverError, ReceiverVerifyError, SenderError};
pub use receiver::Receiver;
pub use sender::Sender;
pub use shared_receiver::SharedReceiver;
pub use shared_sender::SharedSender;

pub(crate) use receiver::StateError as ReceiverStateError;
pub(crate) use sender::StateError as SenderStateError;

pub use mpz_ot_core::kos::{
    msgs, PayloadRecord, ReceiverConfig, ReceiverConfigBuilder, ReceiverConfigBuilderError,
    ReceiverKeys, SenderConfig, SenderConfigBuilder, SenderConfigBuilderError, SenderKeys,
};

// If we're testing we use a smaller chunk size to make sure the chunking code paths are tested.
cfg_if::cfg_if! {
    if #[cfg(test)] {
        pub(crate) const EXTEND_CHUNK_SIZE: usize = 1024;
    } else {
        /// The size of the chunks used to send the extension matrix, 4MB.
        pub(crate) const EXTEND_CHUNK_SIZE: usize = 4 * 1024 * 1024;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    use futures::TryFutureExt;
    use itybity::ToBits;
    use mpz_common::{executor::test_st_executor, Context};
    use mpz_core::Block;
    use rand::Rng;
    use rand_chacha::ChaCha12Rng;
    use rand_core::SeedableRng;

    use crate::{
        ideal::ot::{ideal_ot, IdealOTReceiver, IdealOTSender},
        CommittedOTSender, OTError, OTReceiver, OTSender, OTSetup, RandomOTReceiver,
        RandomOTSender, VerifiableOTReceiver,
    };

    #[fixture]
    fn choices() -> Vec<bool> {
        let mut rng = ChaCha12Rng::seed_from_u64(0);
        (0..128).map(|_| rng.gen()).collect()
    }

    #[fixture]
    fn data() -> Vec<[Block; 2]> {
        let mut rng = ChaCha12Rng::seed_from_u64(0);
        (0..128)
            .map(|_| [rng.gen::<[u8; 16]>().into(), rng.gen::<[u8; 16]>().into()])
            .collect()
    }

    fn choose<T>(
        data: impl Iterator<Item = [T; 2]>,
        choices: impl Iterator<Item = bool>,
    ) -> impl Iterator<Item = T> {
        data.zip(choices)
            .map(|([zero, one], choice)| if choice { one } else { zero })
    }

    async fn setup<Ctx: Context>(
        sender_config: SenderConfig,
        receiver_config: ReceiverConfig,
        ctx_sender: &mut Ctx,
        ctx_receiver: &mut Ctx,
        count: usize,
    ) -> (
        Sender<IdealOTReceiver<Block>>,
        Receiver<IdealOTSender<[Block; 2]>>,
    ) {
        let (base_sender, base_receiver) = ideal_ot();

        let mut sender = Sender::new(sender_config, base_receiver);
        let mut receiver = Receiver::new(receiver_config, base_sender);

        tokio::try_join!(sender.setup(ctx_sender), receiver.setup(ctx_receiver)).unwrap();
        tokio::try_join!(
            sender.extend(ctx_sender, count).map_err(OTError::from),
            receiver.extend(ctx_receiver, count).map_err(OTError::from)
        )
        .unwrap();

        (sender, receiver)
    }

    #[rstest]
    #[tokio::test]
    async fn test_kos(data: Vec<[Block; 2]>, choices: Vec<bool>) {
        let (mut ctx_sender, mut ctx_receiver) = test_st_executor(8);
        let (mut sender, mut receiver) = setup(
            SenderConfig::default(),
            ReceiverConfig::default(),
            &mut ctx_sender,
            &mut ctx_receiver,
            data.len(),
        )
        .await;

        let (output_sender, output_receiver) = tokio::try_join!(
            OTSender::<_, [Block; 2]>::send(&mut sender, &mut ctx_sender, &data)
                .map_err(OTError::from),
            OTReceiver::<_, bool, Block>::receive(&mut receiver, &mut ctx_receiver, &choices)
                .map_err(OTError::from)
        )
        .unwrap();

        let expected = choose(data.iter().copied(), choices.iter_lsb0()).collect::<Vec<_>>();

        assert_eq!(output_sender.id, output_receiver.id);
        assert_eq!(output_receiver.msgs, expected);
    }

    #[tokio::test]
    async fn test_kos_random() {
        let (mut ctx_sender, mut ctx_receiver) = test_st_executor(8);
        let (mut sender, mut receiver) = setup(
            SenderConfig::default(),
            ReceiverConfig::default(),
            &mut ctx_sender,
            &mut ctx_receiver,
            10,
        )
        .await;

        let (output_sender, output_receiver) = tokio::try_join!(
            RandomOTSender::<_, [Block; 2]>::send_random(&mut sender, &mut ctx_sender, 10),
            RandomOTReceiver::<_, bool, Block>::receive_random(
                &mut receiver,
                &mut ctx_receiver,
                10
            )
        )
        .unwrap();

        let expected = output_sender
            .msgs
            .into_iter()
            .zip(output_receiver.choices)
            .map(|(output, choice)| output[choice as usize])
            .collect::<Vec<_>>();

        assert_eq!(output_sender.id, output_receiver.id);
        assert_eq!(output_receiver.msgs, expected);
    }

    #[rstest]
    #[tokio::test]
    async fn test_kos_bytes(data: Vec<[Block; 2]>, choices: Vec<bool>) {
        let (mut ctx_sender, mut ctx_receiver) = test_st_executor(8);
        let (mut sender, mut receiver) = setup(
            SenderConfig::default(),
            ReceiverConfig::default(),
            &mut ctx_sender,
            &mut ctx_receiver,
            data.len(),
        )
        .await;

        let data: Vec<_> = data
            .into_iter()
            .map(|[a, b]| [a.to_bytes(), b.to_bytes()])
            .collect();

        let (output_sender, output_receiver) = tokio::try_join!(
            OTSender::<_, [[u8; 16]; 2]>::send(&mut sender, &mut ctx_sender, &data)
                .map_err(OTError::from),
            OTReceiver::<_, bool, [u8; 16]>::receive(&mut receiver, &mut ctx_receiver, &choices)
                .map_err(OTError::from)
        )
        .unwrap();

        let expected = choose(data.iter().copied(), choices.iter_lsb0()).collect::<Vec<_>>();

        assert_eq!(output_sender.id, output_receiver.id);
        assert_eq!(output_receiver.msgs, expected);
    }

    #[rstest]
    #[tokio::test]
    async fn test_kos_committed_sender(data: Vec<[Block; 2]>, choices: Vec<bool>) {
        let (mut ctx_sender, mut ctx_receiver) = test_st_executor(8);
        let (mut sender, mut receiver) = setup(
            SenderConfig::builder().sender_commit().build().unwrap(),
            ReceiverConfig::builder().sender_commit().build().unwrap(),
            &mut ctx_sender,
            &mut ctx_receiver,
            data.len(),
        )
        .await;

        let (output_sender, output_receiver) = tokio::try_join!(
            OTSender::<_, [Block; 2]>::send(&mut sender, &mut ctx_sender, &data)
                .map_err(OTError::from),
            OTReceiver::<_, bool, Block>::receive(&mut receiver, &mut ctx_receiver, &choices)
                .map_err(OTError::from)
        )
        .unwrap();

        let expected = choose(data.iter().copied(), choices.iter_lsb0()).collect::<Vec<_>>();

        assert_eq!(output_sender.id, output_receiver.id);
        assert_eq!(output_receiver.msgs, expected);

        tokio::try_join!(
            CommittedOTSender::reveal(&mut sender, &mut ctx_sender),
            receiver.accept_reveal(&mut ctx_receiver)
        )
        .unwrap();

        receiver
            .verify(&mut ctx_receiver, output_receiver.id, &data)
            .await
            .unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_shared_kos(data: Vec<[Block; 2]>, choices: Vec<bool>) {
        let (mut ctx_sender, mut ctx_receiver) = test_st_executor(8);
        let (sender, receiver) = setup(
            SenderConfig::default(),
            ReceiverConfig::default(),
            &mut ctx_sender,
            &mut ctx_receiver,
            data.len(),
        )
        .await;

        let mut receiver = SharedReceiver::new(receiver);
        let mut sender = SharedSender::new(sender);

        let (output_sender, output_receiver) = tokio::try_join!(
            OTSender::<_, [Block; 2]>::send(&mut sender, &mut ctx_sender, &data)
                .map_err(OTError::from),
            OTReceiver::<_, bool, Block>::receive(&mut receiver, &mut ctx_receiver, &choices)
                .map_err(OTError::from)
        )
        .unwrap();

        let expected = choose(data.iter().copied(), choices.iter_lsb0()).collect::<Vec<_>>();

        assert_eq!(output_sender.id, output_receiver.id);
        assert_eq!(output_receiver.msgs, expected);
    }
}
