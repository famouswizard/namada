use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::str::FromStr;

use ibc::apps::transfer;
use ibc::core::channel::types::timeout::{TimeoutHeight, TimeoutTimestamp};
use ibc::core::host::types::identifiers::{ChannelId, PortId};
use ibc::primitives::Signer;
use namada_core::address::{self, Address};
use namada_core::borsh::BorshSerializeExt;
use namada_core::chain::{BlockHeader, BlockHeight, ChainId, Epoch, Epochs};
use namada_core::hash::Sha256Hasher;
use namada_events::{EmitEvents, EventToEmit};
use namada_gas::Gas;
use namada_state::mockdb::MockDB;
use namada_state::{
    impl_storage_read, impl_storage_write, iter_prefix_post, iter_prefix_pre,
    write_log, FullAccessState, PrefixIter, State, StateRead,
};
use namada_storage::{
    DBIter, ResultExt, StorageHasher, StorageRead, StorageWrite, DB,
};
use namada_tx_env::TxEnv;
use {
    namada_parameters as parameters, namada_state as state,
    namada_storage as storage, namada_token as token,
};

use crate::{
    IbcActions, IbcCommonContext, IbcMsgTransfer, IbcStorageContext,
    MsgTransfer, TransferModule,
};

#[test]
fn test_transfer() {
    // 1. Setup 2 Namada chains with unique IDs
    let chain_a = TestCtx {
        chain_id: ChainId("chain-a".to_string()),
        state: FullAccessState::default(),
        verifiers: Rc::new(RefCell::new(BTreeSet::new())),
    };

    let chain_b = TestCtx {
        chain_id: ChainId("chain-b".to_string()),
        state: FullAccessState::default(),
        verifiers: Rc::new(RefCell::new(BTreeSet::new())),
    };

    // TODO: Setup and run RPCs to be used by Hermes for the chains

    // 2. Configure hermes

    // 3. Add wallet keys in Hermes for the chains

    // 4. Setup a Hermes channel between the chains

    // 5. Make a transfer
    let verifiers_a = Rc::new(RefCell::new(BTreeSet::new()));
    let mut chain_a_actions =
        IbcActions::<_, parameters::Store<_>, token::Store<TestCtx>>::new(
            Rc::new(RefCell::new(chain_a)),
            verifiers_a,
        );

    let ibc_denom = "NAM";
    let token = transfer::types::PrefixedCoin {
        denom: ibc_denom.parse().unwrap(),
        amount: transfer::types::Amount::from_str("1").unwrap(),
    };
    let sender = address::testing::established_address_1();
    let packet_data = transfer::types::packet::PacketData {
        token,
        // Note: Fails to decode if the sender isn't transparent address
        sender: Signer::from(sender.to_string()),
        receiver: Signer::from("TODO: RECEIVER".to_string()),
        memo: transfer::types::Memo::from("memo"),
    };
    let port_id_on_a = PortId::transfer();
    let chan_id_on_a = ChannelId::zero();
    let message = IbcMsgTransfer {
        port_id_on_a,
        chan_id_on_a,
        packet_data,
        timeout_height_on_b: TimeoutHeight::Never,
        // Note: Fails to decode without a timeout
        timeout_timestamp_on_b: TimeoutTimestamp::from_nanoseconds(
            1_000_000_000_000,
        ),
    };
    let transfer: Option<token::Transfer> = None;
    let data = MsgTransfer { message, transfer }.serialize_to_vec();
    use borsh::BorshDeserialize;
    MsgTransfer::<token::Transfer>::try_from_slice(&data).unwrap();

    chain_a_actions.execute::<token::Transfer>(&data).unwrap();
}

type TestCtx = GenericTestCtx<MockDB, Sha256Hasher>;

#[derive(Debug)]
struct GenericTestCtx<D, H>
where
    D: DB + for<'a> DBIter<'a>,
    H: StorageHasher,
{
    /// The id of the current chain
    pub chain_id: ChainId,
    /// The persistent storage with write log
    pub state: FullAccessState<D, H>,
    pub verifiers: Rc<RefCell<BTreeSet<Address>>>,
}

impl<D, H> IbcStorageContext for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'a> DBIter<'a>,
    H: 'static + StorageHasher,
{
    type Storage = Self;

    fn storage(&self) -> &Self::Storage {
        self
    }

    fn storage_mut(&mut self) -> &mut Self::Storage {
        self
    }

    fn emit_ibc_event(
        &mut self,
        event: crate::event::IbcEvent,
    ) -> state::Result<()> {
        <Self as TxEnv>::emit_event(self, event)
    }

    fn transfer_token(
        &mut self,
        src: &Address,
        dest: &Address,
        token: &Address,
        amount: token::Amount,
    ) -> state::Result<()> {
        token::transfer(self, src, dest, token, amount)
    }

    fn mint_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: token::Amount,
    ) -> state::Result<()> {
        crate::storage::mint_tokens::<_, token::Store<_>>(
            self, target, token, amount,
        )
    }

    fn burn_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: token::Amount,
    ) -> state::Result<()> {
        crate::storage::burn_tokens::<_, token::Store<_>>(
            self, target, token, amount,
        )
    }

    fn insert_verifier(&mut self, verifier: &Address) -> state::Result<()> {
        TxEnv::insert_verifier(self, verifier)
    }

    fn log_string(&self, message: String) {
        println!("{message}");
    }
}

impl<D, H> IbcCommonContext for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'a> DBIter<'a>,
    H: 'static + StorageHasher,
{
}

impl_storage_read!(GenericTestCtx<D, H>);
impl_storage_write!(GenericTestCtx<D, H>);

impl<D, H> StateRead for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'a> DBIter<'a>,
    H: 'static + StorageHasher,
{
    type D = D;
    type H = H;

    fn write_log(&self) -> &state::write_log::WriteLog {
        self.state.write_log()
    }

    fn db(&self) -> &Self::D {
        self.state.db()
    }

    fn in_mem(&self) -> &state::InMemory<Self::H> {
        self.state.in_mem()
    }

    fn charge_gas(&self, gas: Gas) -> state::Result<()> {
        self.state.charge_gas(gas)
    }
}

impl<D, H> State for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'a> DBIter<'a>,
    H: 'static + StorageHasher,
{
    fn write_log_mut(&mut self) -> &mut state::write_log::WriteLog {
        self.state.write_log_mut()
    }

    fn split_borrow(
        &mut self,
    ) -> (
        &mut state::write_log::WriteLog,
        &state::InMemory<Self::H>,
        &Self::D,
    ) {
        self.state.split_borrow()
    }
}

impl<D, H> TxEnv for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'a> DBIter<'a>,
    H: 'static + StorageHasher,
{
    fn read_bytes_temp(
        &self,
        key: &namada_core::storage::Key,
    ) -> state::Result<Option<Vec<u8>>> {
        let (val, gas) = self.state.write_log().read_temp(key)?;
        Ok(val.cloned())
    }

    fn write_bytes_temp(
        &mut self,
        key: &namada_core::storage::Key,
        val: impl AsRef<[u8]>,
    ) -> state::Result<()> {
        self.state
            .write_log_mut()
            .write_temp(key, val.as_ref().to_vec())?;
        Ok(())
    }

    fn insert_verifier(&mut self, addr: &Address) -> state::Result<()> {
        self.verifiers.borrow_mut().insert(addr.clone());
        Ok(())
    }

    fn init_account(
        &mut self,
        code_hash: impl AsRef<[u8]>,
        code_tag: &Option<String>,
        entropy_source: &[u8],
    ) -> state::Result<Address> {
        todo!()
    }

    fn update_validity_predicate(
        &mut self,
        addr: &Address,
        code: impl AsRef<[u8]>,
        code_tag: &Option<String>,
    ) -> state::Result<()> {
        todo!()
    }

    fn emit_event<E: namada_tx_env::EventToEmit>(
        &mut self,
        event: E,
    ) -> state::Result<()> {
        todo!()
    }

    fn charge_gas(&mut self, used_gas: u64) -> state::Result<()> {
        todo!()
    }

    fn get_events(
        &self,
        event_type: &namada_vp::EventType,
    ) -> state::Result<Vec<namada_vp::Event>> {
        todo!()
    }

    fn set_commitment_sentinel(&mut self) {
        todo!()
    }

    fn update_masp_note_commitment_tree(
        transaction: &token::MaspTransaction,
    ) -> state::Result<bool> {
        todo!()
    }
}

impl<D, H> EmitEvents for GenericTestCtx<D, H>
where
    D: 'static + DB + for<'iter> DBIter<'iter>,
    H: 'static + StorageHasher,
{
    #[inline]
    fn emit<E>(&mut self, event: E)
    where
        E: EventToEmit,
    {
        self.write_log_mut().emit_event(event);
    }

    fn emit_many<B, E>(&mut self, event_batch: B)
    where
        B: IntoIterator<Item = E>,
        E: EventToEmit,
    {
        for event in event_batch {
            self.emit(event.into());
        }
    }
}
