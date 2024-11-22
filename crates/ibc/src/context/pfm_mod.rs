//! Implementation of Packet Forward Middleware for our IBC modules.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::rc::Rc;

use ibc::apps::transfer::context::TokenTransferExecutionContext;
use ibc::apps::transfer::handler::send_transfer_execute;
use ibc::apps::transfer::types::msgs::transfer::MsgTransfer;
use ibc::apps::transfer::types::packet::PacketData;
use ibc::apps::transfer::types::{PrefixedDenom, TracePrefix};
use ibc::core::channel::handler::{
    commit_packet_acknowledgment, emit_packet_acknowledgement_event,
};
use ibc::core::channel::types::acknowledgement::Acknowledgement;
use ibc::core::channel::types::channel::{Counterparty, Order};
use ibc::core::channel::types::error::{ChannelError, PacketError};
use ibc::core::channel::types::packet::Packet;
use ibc::core::channel::types::timeout::TimeoutTimestamp;
use ibc::core::channel::types::Version;
use ibc::core::host::types::identifiers::{
    ChannelId, ConnectionId, PortId, Sequence,
};
use ibc::core::router::module::Module;
use ibc::core::router::types::module::{ModuleExtras, ModuleId};
use ibc::primitives::Signer;
use ibc_middleware_packet_forward::{
    InFlightPacket, InFlightPacketKey, PacketForwardMiddleware, PfmContext,
};
use namada_core::address::{Address, InternalAddress, MULTITOKEN};
use namada_core::borsh::BorshSerializeExt;
use namada_core::storage::{self, KeySeg};
use namada_state::{StorageRead, StorageWrite};

use crate::context::transfer_mod::TransferModule;
use crate::context::IbcContext;
use crate::trace::is_sender_chain_source;
use crate::{
    Error, IbcCommonContext, IbcStorageContext, ModuleWrapper,
    TokenTransferContext,
};

const MIDDLEWARES_SUBKEY: &str = "middlewarez";
const PFM_SUBKEY: &str = "pfm";

/// Get the Namada storage key associated to the provided `InFlightPacketKey`.
pub fn get_inflight_packet_key(
    inflight_packet_key: &InFlightPacketKey,
) -> storage::Key {
    let key: storage::Key =
        Address::Internal(InternalAddress::Ibc).to_db_key().into();
    key.with_segment(MIDDLEWARES_SUBKEY.to_string())
        .with_segment(PFM_SUBKEY.to_string())
        .with_segment(inflight_packet_key.port.to_string())
        .with_segment(inflight_packet_key.channel.to_string())
        .with_segment(inflight_packet_key.sequence.to_string())
}

/// A wrapper around an IBC transfer module necessary to
/// build execution contexts. This allows us to implement
/// packet forward middleware on this struct.
pub struct PfmTransferModule<C, Params>
where
    C: IbcCommonContext + Debug,
{
    /// The main module
    pub transfer_module: TransferModule<C>,
    #[allow(missing_docs)]
    pub _phantom: PhantomData<Params>,
}

impl<C, Params> PfmTransferModule<C, Params>
where
    C: IbcCommonContext + Debug,
{
    /// Create a new [`PfmTransferModule`]
    pub fn wrap(
        ctx: Rc<RefCell<C>>,
        verifiers: Rc<RefCell<BTreeSet<Address>>>,
    ) -> PacketForwardMiddleware<Self> {
        PacketForwardMiddleware::next(Self {
            transfer_module: TransferModule::new(ctx, verifiers),
            _phantom: Default::default(),
        })
    }
}

impl<C: IbcCommonContext + Debug, Params> Debug
    for PfmTransferModule<C, Params>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PfmTransferModule")
            .field("transfer_module", &self.transfer_module)
            .finish()
    }
}

impl<C, Params> Module for PfmTransferModule<C, Params>
where
    C: IbcCommonContext + Debug,
{
    fn on_chan_open_init_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<Version, ChannelError> {
        self.transfer_module.on_chan_open_init_validate(
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
    }

    fn on_chan_open_init_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        version: &Version,
    ) -> Result<(ModuleExtras, Version), ChannelError> {
        self.transfer_module.on_chan_open_init_execute(
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            version,
        )
    }

    fn on_chan_open_try_validate(
        &self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &Version,
    ) -> Result<Version, ChannelError> {
        self.transfer_module.on_chan_open_try_validate(
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
    }

    fn on_chan_open_try_execute(
        &mut self,
        order: Order,
        connection_hops: &[ConnectionId],
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty: &Counterparty,
        counterparty_version: &Version,
    ) -> Result<(ModuleExtras, Version), ChannelError> {
        self.transfer_module.on_chan_open_try_execute(
            order,
            connection_hops,
            port_id,
            channel_id,
            counterparty,
            counterparty_version,
        )
    }

    fn on_chan_open_ack_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &Version,
    ) -> Result<(), ChannelError> {
        self.transfer_module.on_chan_open_ack_validate(
            port_id,
            channel_id,
            counterparty_version,
        )
    }

    fn on_chan_open_ack_execute(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
        counterparty_version: &Version,
    ) -> Result<ModuleExtras, ChannelError> {
        self.transfer_module.on_chan_open_ack_execute(
            port_id,
            channel_id,
            counterparty_version,
        )
    }

    fn on_chan_open_confirm_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        self.transfer_module
            .on_chan_open_confirm_validate(port_id, channel_id)
    }

    fn on_chan_open_confirm_execute(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        self.transfer_module
            .on_chan_open_confirm_execute(port_id, channel_id)
    }

    fn on_chan_close_init_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        self.transfer_module
            .on_chan_close_init_validate(port_id, channel_id)
    }

    fn on_chan_close_init_execute(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        self.transfer_module
            .on_chan_close_init_execute(port_id, channel_id)
    }

    fn on_chan_close_confirm_validate(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<(), ChannelError> {
        self.transfer_module
            .on_chan_close_confirm_validate(port_id, channel_id)
    }

    fn on_chan_close_confirm_execute(
        &mut self,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<ModuleExtras, ChannelError> {
        self.transfer_module
            .on_chan_close_confirm_execute(port_id, channel_id)
    }

    fn on_recv_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Option<Acknowledgement>) {
        self.transfer_module.on_recv_packet_execute(packet, relayer)
    }

    fn on_acknowledgement_packet_validate(
        &self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        self.transfer_module.on_acknowledgement_packet_validate(
            packet,
            acknowledgement,
            relayer,
        )
    }

    fn on_acknowledgement_packet_execute(
        &mut self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        self.transfer_module.on_acknowledgement_packet_execute(
            packet,
            acknowledgement,
            relayer,
        )
    }

    fn on_timeout_packet_validate(
        &self,
        packet: &Packet,
        relayer: &Signer,
    ) -> Result<(), PacketError> {
        self.transfer_module
            .on_timeout_packet_validate(packet, relayer)
    }

    fn on_timeout_packet_execute(
        &mut self,
        packet: &Packet,
        relayer: &Signer,
    ) -> (ModuleExtras, Result<(), PacketError>) {
        self.transfer_module
            .on_timeout_packet_execute(packet, relayer)
    }
}

impl<C, Params> PfmContext for PfmTransferModule<C, Params>
where
    C: IbcCommonContext + Debug,
    Params: namada_systems::parameters::Read<<C as IbcStorageContext>::Storage>,
{
    type Error = crate::Error;

    fn send_transfer_execute(
        &mut self,
        msg: MsgTransfer,
    ) -> Result<Sequence, Self::Error> {
        let seq = self
            .transfer_module
            .ctx
            .inner
            .borrow()
            .get_next_sequence_send(&msg.port_id_on_a, &msg.chan_id_on_a)
            .map_err(|e| Error::Context(Box::new(e)))?;

        let mut ctx = IbcContext::<C, Params>::new(
            self.transfer_module.ctx.inner.clone(),
        );
        let mut token_transfer_ctx = TokenTransferContext::new(
            self.transfer_module.ctx.inner.clone(),
            Default::default(),
        );

        send_transfer_execute(&mut ctx, &mut token_transfer_ctx, msg)
            .map_err(Error::TokenTransfer)?;
        Ok(seq)
    }

    fn receive_refund_execute(
        &mut self,
        _: &Packet,
        _: PacketData,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send_refund_execute(
        &mut self,
        msg: &InFlightPacket,
    ) -> Result<(), Self::Error> {
        // the token is not Namada native
        if is_sender_chain_source(
            msg.packet_data.token.to_string(),
            &msg.packet_src_port_id,
            &msg.packet_src_channel_id,
        ) {
            let escrow_address = Address::Internal(InternalAddress::Ibc);
            self.transfer_module.ctx.insert_verifier(&MULTITOKEN);
            let mut token_transfer_ctx = TokenTransferContext::new(
                self.transfer_module.ctx.inner.clone(),
                self.transfer_module.ctx.verifiers.clone(),
            );

            token_transfer_ctx
                .burn_coins_execute(
                    &escrow_address,
                    &msg.packet_data.token,
                    &msg.packet_data.memo,
                )
                .map_err(Error::TokenTransfer)
        } else {
            Ok(())
        }
    }

    fn write_ack_and_events(
        &mut self,
        packet: &Packet,
        acknowledgement: &Acknowledgement,
    ) -> Result<(), Self::Error> {
        let mut ctx = IbcContext::<C, Params>::new(
            self.transfer_module.ctx.inner.clone(),
        );
        commit_packet_acknowledgment(&mut ctx, packet, acknowledgement)
            .map_err(|e| Error::Context(Box::new(e)))?;
        emit_packet_acknowledgement_event(
            &mut ctx,
            packet.clone(),
            acknowledgement.clone(),
        )
        .map_err(|e| Error::Context(Box::new(e)))
    }

    fn override_receiver(
        &self,
        _: &ChannelId,
        _: &Signer,
    ) -> Result<Signer, Self::Error> {
        Ok(Address::Internal(InternalAddress::Ibc).to_string().into())
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn timeout_timestamp(
        &self,
        timeout_duration: dur::Duration,
    ) -> Result<TimeoutTimestamp, Self::Error> {
        let timestamp = self
            .transfer_module
            .ctx
            .inner
            .borrow()
            .host_timestamp()
            .map_err(|e| Error::Other(e.to_string()))?
            + timeout_duration.try_to_std().ok_or_else(|| {
                Error::Other(format!(
                    "Packet timeout duration is too large: {timeout_duration}"
                ))
            })?;
        timestamp
            .map(TimeoutTimestamp::At)
            .map_err(|e| Error::Other(e.to_string()))
    }

    fn store_inflight_packet(
        &mut self,
        key: InFlightPacketKey,
        inflight_packet: InFlightPacket,
    ) -> Result<(), Self::Error> {
        let mut ctx = self.transfer_module.ctx.inner.borrow_mut();
        let key = get_inflight_packet_key(&key);
        ctx.storage_mut()
            .write(&key, inflight_packet.serialize_to_vec())
            .map_err(Error::Storage)
    }

    fn retrieve_inflight_packet(
        &self,
        key: &InFlightPacketKey,
    ) -> Result<Option<InFlightPacket>, Self::Error> {
        let mut ctx = self.transfer_module.ctx.inner.borrow_mut();
        let key = get_inflight_packet_key(key);
        ctx.storage_mut().read(&key).map_err(Error::Storage)
    }

    fn delete_inflight_packet(
        &mut self,
        key: &InFlightPacketKey,
    ) -> Result<(), Self::Error> {
        let mut ctx = self.transfer_module.ctx.inner.borrow_mut();
        let key = get_inflight_packet_key(key);
        ctx.storage_mut().delete(&key).map_err(Error::Storage)
    }

    fn get_denom_for_this_chain(
        &self,
        this_chain_port: &PortId,
        this_chain_chan: &ChannelId,
        source_port: &PortId,
        source_chan: &ChannelId,
        source_denom: &PrefixedDenom,
    ) -> Result<PrefixedDenom, Self::Error> {
        let mut new_denom = source_denom.clone();
        // We will see this prefix on the asset if it is Namada native
        let native_prefix =
            TracePrefix::new(source_port.clone(), source_chan.clone());
        if source_denom.trace_path.starts_with(&native_prefix) {
            // this asset originated from this chain; unwind
            new_denom.trace_path.remove_prefix(&native_prefix);
        } else {
            // this asset originated from a foreign chain; add a new hop
            new_denom.trace_path.add_prefix(TracePrefix::new(
                this_chain_port.clone(),
                this_chain_chan.clone(),
            ));
        }
        Ok(new_denom)
    }
}

impl<T> ModuleWrapper for PacketForwardMiddleware<T>
where
    T: Module + PfmContext,
{
    fn as_module(&self) -> &dyn Module {
        self
    }

    fn as_module_mut(&mut self) -> &mut dyn Module {
        self
    }

    fn module_id(&self) -> ModuleId {
        ModuleId::new(ibc::apps::transfer::types::MODULE_ID_STR.to_string())
    }

    fn port_id(&self) -> PortId {
        PortId::transfer()
    }
}
